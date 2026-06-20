mod api;
mod config;
mod monitor;

use anyhow::Context;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::api::AppState;
use crate::config::{Config, LdapTargetConfig, SearchCheckConfig, SearchScope};
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

    // 2. Determine configuration file path
    let args: Vec<String> = std::env::args().collect();
    let config_path = if args.len() > 1 {
        args[1].clone()
    } else {
        if std::path::Path::new("config.yaml").exists() {
            "config.yaml".to_string()
        } else if std::path::Path::new("config.json").exists() {
            "config.json".to_string()
        } else {
            "config.yaml".to_string()
        }
    };

    // 3. Load or generate default configuration
    let config = if !std::path::Path::new(&config_path).exists() {
        tracing::info!(
            "Config file not found. Generating default template at '{}'",
            config_path
        );
        generate_default_config(&config_path)?
    } else {
        tracing::info!("Loading config from '{}'", config_path);
        let content = std::fs::read(&config_path)
            .with_context(|| format!("Failed to read config file at '{}'", config_path))?;
        let path_lower = config_path.to_lowercase();
        if path_lower.ends_with(".yaml") || path_lower.ends_with(".yml") {
            serde_yaml::from_slice(&content)
                .with_context(|| format!("Failed to parse YAML config from '{}'", config_path))?
        } else {
            serde_json::from_slice(&content)
                .with_context(|| format!("Failed to parse JSON config from '{}'", config_path))?
        }
    };

    let bind_address = config.bind_address.clone();
    let config_arc = Arc::new(RwLock::new(config));

    // 4. Initialize and start monitors
    let monitor_manager = Arc::new(MonitorManager::new());
    {
        let config_read = config_arc.read().await;
        monitor_manager.update_monitors(&config_read).await;
    }

    // 5. Build application state and router
    let state = Arc::new(AppState {
        config: config_arc,
        monitor_manager,
        config_path,
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

fn generate_default_config(path: &str) -> Result<Config, anyhow::Error> {
    let default_cfg = Config {
        bind_address: "0.0.0.0:8080".to_string(),
        ldaps: vec![LdapTargetConfig {
            id: "local_ldap_sample".to_string(),
            url: "ldap://localhost:389".to_string(),
            bind_dn: Some("cn=admin,dc=example,dc=com".to_string()),
            bind_password: Some("adminpassword".to_string()),
            bind_interval_secs: 10,
            search_interval_secs: 10,
            timeout_secs: 5,
            search_check: Some(SearchCheckConfig {
                base: "dc=example,dc=com".to_string(),
                filter: "(objectClass=*)".to_string(),
                scope: SearchScope::Subtree,
            }),
        }],
    };

    let content = if path.ends_with(".yaml") || path.ends_with(".yml") {
        serde_yaml::to_string(&default_cfg)?
    } else {
        serde_json::to_string_pretty(&default_cfg)?
    };

    std::fs::write(path, content)?;
    Ok(default_cfg)
}
