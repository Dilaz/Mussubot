use crate::components::ComponentManager;
use crate::components::redis_service::RedisActorHandle;
use std::sync::Arc;
use tokio::sync::oneshot;
use tracing::{error, info};

#[cfg(unix)]
use tokio::signal::unix::{signal, SignalKind};
#[cfg(windows)]
use tokio::signal::windows::{ctrl_c, ctrl_break};

/// Set up signal handlers for graceful shutdown
pub async fn handle_signals(
    shutdown_send: oneshot::Sender<()>,
    component_manager: Arc<ComponentManager>,
    redis_handle: RedisActorHandle,
) {
    // Wait for a termination signal
    wait_for_signal().await;
    
    // Shut down all components
    if let Err(e) = component_manager.shutdown_all().await {
        error!("Error shutting down components: {:?}", e);
    } else {
        info!("All components shut down successfully");
    }
    
    // Shut down Redis actor
    if let Err(e) = redis_handle.shutdown().await {
        error!("Error shutting down Redis actor: {:?}", e);
    } else {
        info!("Redis actor shut down successfully");
    }
    
    // Send shutdown signal to main task
    let _ = shutdown_send.send(());
}

/// Platform-specific signal handling implementation
#[cfg(unix)]
async fn wait_for_signal() {
    // Handle SIGTERM (sent by Kubernetes when pod is terminating)
    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to create SIGTERM signal handler");
    // Handle SIGINT (Ctrl+C)
    let mut sigint = signal(SignalKind::interrupt()).expect("Failed to create SIGINT signal handler");
    
    tokio::select! {
        _ = sigterm.recv() => {
            info!("Received SIGTERM signal, initiating graceful shutdown");
        }
        _ = sigint.recv() => {
            info!("Received SIGINT signal, initiating graceful shutdown");
        }
    }
}

/// Platform-specific signal handling implementation
#[cfg(windows)]
async fn wait_for_signal() {
    // Handle Ctrl+C
    let mut ctrlc = ctrl_c().expect("Failed to create Ctrl+C signal handler");
    // Handle Ctrl+Break
    let mut ctrlbreak = ctrl_break().expect("Failed to create Ctrl+Break signal handler");
    
    tokio::select! {
        _ = ctrlc.recv() => {
            info!("Received Ctrl+C signal, initiating graceful shutdown");
        }
        _ = ctrlbreak.recv() => {
            info!("Received Ctrl+Break signal, initiating graceful shutdown");
        }
    }
} 