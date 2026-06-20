use axum::{
    Router,
    extract::{State, Query},
    http::StatusCode,
    response::IntoResponse,
    response::Json,
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

use crate::config::Config;
use crate::monitor::MonitorManager;

pub struct AppState {
    pub config: Arc<RwLock<Config>>,
    pub monitor_manager: Arc<MonitorManager>,
    pub config_path: String,
    pub start_time: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct StatusParams {
    #[serde(default)]
    pub verbose: bool,
}

#[derive(Serialize)]
struct OTelHealthResponse {
    start_time: DateTime<Utc>,
    healthy: bool,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    status_time: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    components: Option<std::collections::HashMap<String, OTelComponentStatus>>,
}

#[derive(Serialize)]
struct OTelComponentStatus {
    healthy: bool,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    status_time: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    attributes: Option<serde_json::Value>,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/config", get(get_config).post(update_config))
        .route("/status", get(get_status))
        .with_state(state)
}

async fn health_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let (status_code, response) = get_otel_health(&state, false).await;
    (status_code, Json(response))
}

async fn get_config(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let config = state.config.read().await;
    (StatusCode::OK, Json(config.clone()))
}

async fn update_config(
    State(state): State<Arc<AppState>>,
    Json(new_config): Json<Config>,
) -> impl IntoResponse {
    // Serialize to JSON — config-rs can load it back regardless of file extension
    let serialized_content = match serde_json::to_string_pretty(&new_config) {
        Ok(content) => content,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "status": "error", "message": format!("Failed to serialize config: {}", e) })),
            );
        }
    };

    // Persist the configuration to the file on disk
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

async fn get_status(
    State(state): State<Arc<AppState>>,
    Query(params): Query<StatusParams>,
) -> impl IntoResponse {
    let (status_code, response) = get_otel_health(&state, params.verbose).await;
    (status_code, Json(response))
}

async fn get_otel_health(
    state: &Arc<AppState>,
    verbose: bool,
) -> (StatusCode, OTelHealthResponse) {
    let statuses = state.monitor_manager.get_statuses().await;
    
    let mut all_healthy = true;
    let mut has_permanent = false;
    let mut overall_status_time = state.start_time;
    let mut errors = Vec::new();
    let mut components = std::collections::HashMap::new();

    for (id, status) in &statuses {
        let comp_status = classify_status(status);
        if !comp_status.healthy {
            all_healthy = false;
            if comp_status.status == "StatusPermanentError" {
                has_permanent = true;
            }
            if let Some(ref err) = comp_status.error {
                errors.push(format!("{}: {}", id, err));
            }
        }
        overall_status_time = overall_status_time.max(comp_status.status_time);
        
        if verbose {
            components.insert(id.clone(), comp_status);
        }
    }

    // Determine aggregate status
    let (overall_status, overall_error) = if all_healthy {
        ("StatusOK", None)
    } else {
        let joined_errors = errors.join("; ");
        let label = if has_permanent {
            "StatusPermanentError"
        } else {
            "StatusRecoverableError"
        };
        (label, Some(joined_errors))
    };

    let response = OTelHealthResponse {
        start_time: state.start_time,
        healthy: all_healthy,
        status: overall_status,
        error: overall_error,
        status_time: overall_status_time,
        components: if verbose { Some(components) } else { None },
    };

    let status_code = if all_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status_code, response)
}

fn classify_status(status: &crate::monitor::LdapStatus) -> OTelComponentStatus {
    let mut errs = Vec::new();
    let mut is_permanent = false;

    if let Some(ref err) = status.bind_error_message {
        errs.push(format!("Bind: {}", err));
        let err_lower = err.to_lowercase();
        // Classify authentication/credentials issues as permanent, as they require human configuration edits
        if err_lower.contains("credential") || err_lower.contains("password") || err_lower.contains("auth") || err_lower.contains("dn") || err_lower.contains("invalid") {
            is_permanent = true;
        }
    }

    if let Some(ref err) = status.search_error_message {
        errs.push(format!("Search: {}", err));
        let err_lower = err.to_lowercase();
        if err_lower.contains("credential") || err_lower.contains("password") || err_lower.contains("auth") || err_lower.contains("dn") || err_lower.contains("invalid") {
            is_permanent = true;
        }
    }

    let (otel_status, error_str) = if status.up {
        ("StatusOK", None)
    } else {
        let combined_err = if errs.is_empty() {
            "Check failed".to_string()
        } else {
            errs.join("; ")
        };
        
        let label = if is_permanent {
            "StatusPermanentError"
        } else {
            "StatusRecoverableError"
        };
        
        (label, Some(combined_err))
    };

    let status_time = status.last_bind_time
        .max(status.last_search_time)
        .unwrap_or(status.last_bind_time.unwrap_or_else(Utc::now));

    // Populate metadata attributes
    let mut attrs = serde_json::Map::new();
    attrs.insert("url".to_string(), serde_json::Value::String(status.url.clone()));
    if let Some(lat) = status.bind_latency_ms {
        attrs.insert("bind_latency_ms".to_string(), serde_json::Value::Number(lat.into()));
    }
    if let Some(lat) = status.search_latency_ms {
        attrs.insert("search_latency_ms".to_string(), serde_json::Value::Number(lat.into()));
    }

    OTelComponentStatus {
        healthy: status.up,
        status: otel_status,
        error: error_str,
        status_time,
        attributes: Some(serde_json::Value::Object(attrs)),
    }
}
