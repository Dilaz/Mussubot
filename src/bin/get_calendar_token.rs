use mussubotti::components::google_calendar::token::TokenManager;
use mussubotti::components::redis_service::RedisActor;
use mussubotti::config::Config;
use mussubotti::error::{other_error, BotResult};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> BotResult<()> {
    // Load configuration
    let config = Config::load()?;
    let config = Arc::new(RwLock::new(config));

    // Create Redis actor
    let (mut redis_actor, redis_handle) = RedisActor::new(config.clone());

    // Spawn Redis actor task
    let _redis_task = tokio::spawn(async move {
        redis_actor.run().await;
    });

    // Create token manager with Redis handle
    let token_manager = TokenManager::new(config.clone(), redis_handle);

    // Get client ID and secret
    let client_id = config.read().await.google_client_id.clone();
    let client_secret = config.read().await.google_client_secret.clone();

    // Generate random state for security
    let state = uuid::Uuid::new_v4().to_string();

    // Construct authorization URL
    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?\
        client_id={}&\
        redirect_uri=http://localhost:8080&\
        response_type=code&\
        access_type=offline&\
        prompt=consent&\
        scope=https://www.googleapis.com/auth/calendar.readonly&\
        state={}",
        client_id, state
    );

    // Open browser for authorization
    println!("Opening browser for Google Calendar authorization...");
    webbrowser::open(&auth_url)?;

    // Start local server to receive the callback
    let server = tiny_http::Server::http("0.0.0.0:8080")?;
    println!("Waiting for authorization callback...");

    // Handle the callback
    let request = server.recv()?;
    let url = request.url().to_string();

    // Parse the authorization code from the URL
    let code = url
        .split("code=")
        .nth(1)
        .and_then(|s| s.split('&').next())
        .ok_or_else(|| other_error("No authorization code found in callback"))?;

    // Exchange code for tokens
    let token_url = "https://oauth2.googleapis.com/token";
    let client = reqwest::Client::new();

    let response = client
        .post(token_url)
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code.to_string()),
            ("redirect_uri", "http://localhost:8080".to_string()),
            ("grant_type", "authorization_code".to_string()),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        return Err(other_error(&format!("Failed to get token: {}", error_text)));
    }

    let mut token_data: serde_json::Value = response.json().await?;

    // Add expiry timestamp
    let expires_in = token_data
        .get("expires_in")
        .and_then(|v| v.as_i64())
        .unwrap_or(3600);
    let expires_at = chrono::Utc::now().timestamp() + expires_in;

    let token_data = if let Some(obj) = token_data.as_object_mut() {
        obj.insert("expires_at".to_string(), json!(expires_at));
        token_data
    } else {
        return Err(other_error("Token data is not an object"));
    };

    // Save token using TokenManager
    token_manager.set_token(token_data).await?;

    // Send success response to browser
    let response =
        tiny_http::Response::from_string("Authorization successful! You can close this window.");
    request.respond(response)?;

    println!("Token successfully saved to Redis!");

    Ok(())
}
