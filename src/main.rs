mod api;
mod config;
mod monitor;

use anyhow::Context;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::api::AppState;
use crate::config::Config;
use crate::monitor::MonitorManager;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // 1. Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ldapmon=info,info".into()),
        )
        .init();

    tracing::info!("Starting LDAP Monitor...");

    // 2. Parse optional --config <path> flag; fall back to config-rs auto-discovery
    let args: Vec<String> = std::env::args().collect();
    let config_arg: Option<&str> = args
        .windows(2)
        .find(|w| w[0] == "--config")
        .map(|w| w[1].as_str());

    // 3. Load configuration via config-rs (format auto-detected from extension)
    tracing::info!(
        config = config_arg.unwrap_or("<auto-discover>"),
        "Loading configuration"
    );
    let config = Config::load(config_arg)?;

    let bind_address = config.bind_address.clone();
    let config_arc = Arc::new(RwLock::new(config));

    // 4. Initialize and start monitors
    let monitor_manager = Arc::new(MonitorManager::new());
    {
        let config_read = config_arc.read().await;
        monitor_manager.update_monitors(&config_read).await;
    }

    // config_path is used by the REST API to persist updates; default to config.json
    let config_path = config_arg
        .unwrap_or("config.json")
        .to_string();

    // 5. Build application state and router
    let state = Arc::new(AppState {
        config: config_arc,
        monitor_manager,
        config_path,
        start_time: chrono::Utc::now(),
    });

    let app = api::create_router(state);

    // 6. Start the Axum HTTP REST server
    tracing::info!("Starting HTTP REST server on {}...", bind_address);
    let listener = tokio::net::TcpListener::bind(&bind_address)
        .await
        .with_context(|| format!("Failed to bind to HTTP address '{}'", bind_address))?;

    axum::serve(listener, app)
        .await
        .context("Error running Axum server")?;

    Ok(())
}
