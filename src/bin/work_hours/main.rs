#![cfg_attr(not(feature = "web-interface"), allow(dead_code, unused_imports))]

// Import modules
mod auth;
mod db;
mod handlers;
mod model;
mod parser;

use std::sync::Arc;

#[cfg(feature = "web-interface")]
use axum::{
    body::Body,
    extract::DefaultBodyLimit,
    http::Request,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Router,
};
#[cfg(feature = "web-interface")]
use std::net::SocketAddr;
#[cfg(feature = "web-interface")]
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};
#[cfg(feature = "web-interface")]
use tracing::info;
#[cfg(feature = "web-interface")]
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::auth::AuthService;
use crate::db::RedisDB;
use crate::handlers::{
    dashboard_handler, health_handler, index_handler, login_form_handler, login_handler,
    upload_form_handler, upload_handler,
};
use crate::model::WorkHoursDb;

#[derive(Clone)]
pub struct AppState {
    /// Directory where uploaded files are stored
    pub upload_dir: String,
    /// Auth service for JWT operations
    pub auth_service: Arc<AuthService>,
    /// Database for work hours
    pub db: Arc<dyn WorkHoursDb>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(feature = "web-interface"))]
    {
        println!("Web interface feature not enabled. Please compile with --features web-interface");
        return Ok(());
    }

    #[cfg(feature = "web-interface")]
    {
        // Load environment variables
        dotenvy::dotenv().ok();

        // Initialize tracing
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new(
                std::env::var("RUST_LOG").unwrap_or_else(|_| "info,tower_http=debug".into()),
            ))
            .with(tracing_subscriber::fmt::layer())
            .init();

        info!("Starting work hours web server");

        // Setup app state
        let upload_dir = std::env::var("UPLOAD_DIR").unwrap_or_else(|_| "./uploads".to_string());

        // Ensure upload directory exists
        std::fs::create_dir_all(&upload_dir)?;

        let auth_config = auth::AuthConfig::default();
        info!(
            "Using admin credentials from environment: username={}",
            auth_config.admin_username
        );
        let auth_service = Arc::new(AuthService::new(auth_config));

        // Initialize database with direct Redis connection
        let db: Arc<dyn WorkHoursDb> = match RedisDB::new() {
            Ok(redis_db) => {
                info!("Connected to Redis successfully");
                Arc::new(redis_db)
            }
            Err(e) => {
                // Log the error and fall back to a mock implementation
                tracing::error!("Failed to connect to Redis: {}", e);
                #[cfg(not(feature = "web-interface"))]
                panic!("Redis connection failed: {}", e);

                #[cfg(feature = "web-interface")]
                {
                    info!("Using in-memory database as fallback");
                    Arc::new(model::InMemoryDb::default())
                }
            }
        };

        let state = AppState {
            upload_dir,
            auth_service: auth_service.clone(),
            db,
        };

        // Authentication middleware
        async fn auth_middleware(
            req: Request<Body>,
            next: Next,
            auth_service: Arc<AuthService>,
        ) -> Result<Response, Response> {
            // Public routes are always allowed
            let path = req.uri().path();
            if path == "/" || path == "/login" || path.starts_with("/assets") || path == "/health" {
                return Ok(next.run(req).await);
            }

            // Extract parts to use with extract_token
            let (parts, body) = req.into_parts();

            // Use the extract_token function from auth module
            match auth::extract_token(&parts) {
                Ok(token) => {
                    // Validate the token
                    match auth_service.validate_token(&token) {
                        Ok(claims) => {
                            // Create JwtAuth to pass along
                            let auth = auth::JwtAuth { claims };

                            // Reconstruct the request with auth data
                            let mut req = Request::from_parts(parts, body);
                            req.extensions_mut().insert(auth);

                            // User is authenticated, proceed
                            Ok(next.run(req).await)
                        }
                        Err(_) => {
                            // Invalid token, redirect to login
                            Err(Redirect::to("/login").into_response())
                        }
                    }
                }
                Err(_) => {
                    // No token found, redirect to login
                    Err(Redirect::to("/login").into_response())
                }
            }
        }

        // Create middleware with auth service
        let auth_service_for_middleware = auth_service.clone();
        let auth_middleware = move |req: Request<Body>, next: Next| {
            auth_middleware(req, next, auth_service_for_middleware.clone())
        };

        // Build the router
        let app = Router::new()
            .route("/", get(index_handler))
            .route("/login", get(login_form_handler).post(login_handler))
            .route("/health", get(health_handler))
            .route("/upload", get(upload_form_handler).post(upload_handler))
            .route("/dashboard", get(dashboard_handler))
            // Apply auth middleware
            .layer(axum::middleware::from_fn(auth_middleware))
            // Serve static files
            .nest_service("/assets", ServeDir::new("assets"))
            // Other middlewares
            .layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // 10MB limit
            .layer(TraceLayer::new_for_http())
            .layer(CorsLayer::permissive())
            .with_state(state);

        // Bind to address and run server
        let port = std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(3000);
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        info!("Listening on {}", addr);

        // Start the server
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;
    }

    Ok(())
}
