use std::sync::Arc;
use axum::{
    routing::get,
    Router,
    extract::State,
    response::Json,
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::json;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::monitor::MonitorManager;

pub struct AppState {
    pub config: Arc<RwLock<Config>>,
    pub monitor_manager: Arc<MonitorManager>,
    pub config_path: String,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/config", get(get_config).post(update_config))
        .route("/status", get(get_status))
        .with_state(state)
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "UP" })))
}

async fn get_config(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let config = state.config.read().await;
    (StatusCode::OK, Json(config.clone()))
}

async fn update_config(
    State(state): State<Arc<AppState>>,
    Json(new_config): Json<Config>,
) -> impl IntoResponse {
    // 1. Serialize new configuration to match the extension of configuration file
    let path = state.config_path.to_lowercase();
    let serialization_result = if path.ends_with(".yaml") || path.ends_with(".yml") {
        serde_yaml::to_string(&new_config)
            .map_err(|e| format!("Failed to serialize to YAML: {}", e))
    } else {
        serde_json::to_string_pretty(&new_config)
            .map_err(|e| format!("Failed to serialize to JSON: {}", e))
    };

    let serialized_content = match serialization_result {
        Ok(content) => content,
        Err(err_msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "status": "error", "message": err_msg })),
            );
        }
    };

    // 2. Persist the configuration to the file on disk
    if let Err(e) = std::fs::write(&state.config_path, serialized_content) {
        let err_msg = format!("Failed to write configuration file to disk: {}", e);
        tracing::error!("{}", err_msg);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status": "error", "message": err_msg })),
        );
    }

    // 3. Update the shared configuration state in memory
    {
        let mut config_lock = state.config.write().await;
        *config_lock = new_config.clone();
    }

    // 4. Update the active monitoring tasks
    state.monitor_manager.update_monitors(&new_config).await;

    tracing::info!("Configuration successfully updated via REST and monitors reloaded");

    (
        StatusCode::OK,
        Json(json!({
            "status": "success",
            "message": "Configuration updated and monitors reloaded. Note: Any changes to bind_address will take effect only after app restart."
        })),
    )
}

async fn get_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let statuses = state.monitor_manager.get_statuses().await;
    (StatusCode::OK, Json(statuses))
}
