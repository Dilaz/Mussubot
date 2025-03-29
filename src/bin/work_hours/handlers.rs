use axum::{
    extract::{Multipart, State, Form, Extension},
    response::{Html, IntoResponse, Redirect},
    http::{StatusCode, header, Uri},
};
use tracing::{error, info};
use std::collections::HashMap;
use std::env;

use crate::AppState;
use crate::parser::parse_schedule_image;
use crate::auth::{Credentials, AuthError, JwtAuth};

/// Handler for the index page
pub async fn index_handler() -> impl IntoResponse {
    Html(include_str!("../../../assets/work_hours/index.html"))
}

/// Extracts query parameters from the URI
fn get_query_params(uri: Uri) -> HashMap<String, String> {
    let query = uri.query().unwrap_or("");
    query.split('&')
        .filter_map(|pair| {
            let mut parts = pair.split('=');
            match (parts.next(), parts.next()) {
                (Some(key), Some(value)) => Some((
                    key.to_string(),
                    // Try to URL decode the value
                    percent_decode(value)
                )),
                _ => None,
            }
        })
        .collect()
}

/// Simple percent decoding function
fn percent_decode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '%' {
            if let (Some(h1), Some(h2)) = (chars.next(), chars.next()) {
                if let Ok(byte) = u8::from_str_radix(&format!("{}{}", h1, h2), 16) {
                    // Only add ASCII chars and replace non-ASCII with '?'
                    if byte < 128 {
                        result.push(byte as char);
                    } else {
                        result.push('?');
                    }
                } else {
                    result.push('%');
                    result.push(h1);
                    result.push(h2);
                }
            } else {
                result.push('%');
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    
    result
}

/// Simple percent encoding function
fn percent_encode(input: &str) -> String {
    input.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
                c.to_string()
            } else if c == ' ' {
                '+'.to_string()
            } else {
                format!("%{:02X}", c as u8)
            }
        })
        .collect()
}

/// List of acceptable error messages
const ALLOWED_ERROR_MESSAGES: [&str; 2] = [
    "Invalid credentials",
    "Authentication error occurred"
];

/// Handler for the login form page
pub async fn login_form_handler(uri: Uri) -> impl IntoResponse {
    let params = get_query_params(uri);
    let error = params.get("error").cloned();
    
    let html = include_str!("../../../assets/work_hours/login.html");
    let html = if let Some(error_msg) = error {
        // Only display the error if it's in our allowed list
        if ALLOWED_ERROR_MESSAGES.contains(&error_msg.as_str()) {
            html.replace("<!-- ERROR_MESSAGE -->", &format!("<div class=\"bg-red-600 text-white p-4 rounded mb-4\">{}</div>", error_msg))
        } else {
            // If error not in allowed list, don't show anything
            html.to_string()
        }
    } else {
        html.to_string()
    };
    
    Html(html)
}

/// Handler for login form submission
pub async fn login_handler(
    State(state): State<AppState>,
    Form(credentials): Form<Credentials>
) -> impl IntoResponse {
    // Authenticate the user
    match state.auth_service.authenticate(&credentials.username, &credentials.password) {
        Ok(token) => {
            info!("User {} successfully authenticated", credentials.username);
            // Create a response with a redirect and set the auth cookie
            let cookie = format!("auth_token={}; Path=/; HttpOnly; SameSite=Strict", token);
            let mut response = Redirect::to("/upload").into_response();
            response.headers_mut().insert(
                header::SET_COOKIE,
                header::HeaderValue::from_str(&cookie).unwrap(),
            );
            response
        }
        Err(AuthError::Unauthorized) => {
            error!("Failed login attempt for user: {}", credentials.username);
            let encoded_error = percent_encode(ALLOWED_ERROR_MESSAGES[0]);
            let mut response = Redirect::to(&format!("/login?error={}", encoded_error)).into_response();
            response.headers_mut().insert(
                header::SET_COOKIE,
                header::HeaderValue::from_static("auth_token=; Path=/; HttpOnly; Max-Age=0"),
            );
            response
        }
        Err(err) => {
            error!("Authentication error: {:?}", err);
            let encoded_error = percent_encode(ALLOWED_ERROR_MESSAGES[1]);
            let mut response = Redirect::to(&format!("/login?error={}", encoded_error)).into_response();
            response.headers_mut().insert(
                header::SET_COOKIE,
                header::HeaderValue::from_static("auth_token=; Path=/; HttpOnly; Max-Age=0"),
            );
            response
        }
    }
}

/// Handler for the upload form page
pub async fn upload_form_handler(Extension(auth): Extension<JwtAuth>) -> impl IntoResponse {
    // Clone the name first to avoid borrow issues
    let name_for_value = auth.claims.name.clone()
        .unwrap_or_else(|| env::var("DEFAULT_EMPLOYEE_NAME").unwrap_or_else(|_| "".to_string()));
    
    
    let html = include_str!("../../../assets/work_hours/upload.html")
        .replace("value=\"\"", &format!("value=\"{}\"", name_for_value));
    
    Html(html)
}

/// Handler for the dashboard page
pub async fn dashboard_handler(Extension(_auth): Extension<JwtAuth>) -> impl IntoResponse {
    let html = include_str!("../../../assets/work_hours/dashboard.html");
    
    Html(html)
}

/// Get the default employee name from environment
fn get_default_employee_name() -> String {
    env::var("DEFAULT_EMPLOYEE_NAME").unwrap_or_else(|_| "Brian".to_string())
}

/// Handler for file uploads
pub async fn upload_handler(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, StatusCode> {
    let mut name = None;
    let mut schedule_file = None;

    while let Some(field) = multipart.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)? {
        let field_name = field.name().unwrap_or_default().to_string();

        if field_name == "name" {
            if let Ok(value) = field.text().await {
                name = Some(value);
            }
        } else if field_name == "schedule_file" {
            let data = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            schedule_file = Some(data);
        }
    }

    // Use provided name, or fall back to the name from auth token if available,
    // or use the default name as last resort
    let name_val = name.unwrap_or_else(|| {
        auth.claims.name.clone().unwrap_or_else(get_default_employee_name)
    });
    
    // Clone name for logging
    let name_for_log = name_val.clone();
    
    // Process the file and schedule
    if let Some(file_data) = schedule_file {
        // Parse the schedule
        match parse_schedule_image(&name_val, &file_data).await {
            Ok(schedule) => {
                // Store the schedule
                match state.db.set_schedule(&name_val, &schedule).await {
                    Ok(_) => {
                        info!("Schedule for {} processed and stored successfully", name_for_log);
                        Ok(Redirect::to("/dashboard"))
                    }
                    Err(e) => {
                        error!("Failed to store schedule: {}", e);
                        Err(StatusCode::INTERNAL_SERVER_ERROR)
                    }
                }
            }
            Err(e) => {
                error!("Failed to parse schedule: {}", e);
                Err(StatusCode::BAD_REQUEST)
            }
        }
    } else {
        error!("Missing required fields for upload");
        Err(StatusCode::BAD_REQUEST)
    }
}

// Handler for API health check
pub async fn health_handler() -> &'static str {
    "OK"
}
