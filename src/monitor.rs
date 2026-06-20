use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ldap3::LdapConnAsync;

use crate::config::{Config, LdapTargetConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LdapStatus {
    pub id: String,
    pub url: String,
    pub up: bool, // Combined health status: bind_success && search_success

    // Bind check fields
    pub last_bind_time: Option<DateTime<Utc>>,
    pub bind_latency_ms: Option<u64>,
    pub bind_error_message: Option<String>,
    pub bind_success: Option<bool>,

    // Search check fields
    pub last_search_time: Option<DateTime<Utc>>,
    pub search_latency_ms: Option<u64>,
    pub search_error_message: Option<String>,
    pub search_success: Option<bool>,
}

pub struct MonitorManager {
    statuses: Arc<RwLock<HashMap<String, LdapStatus>>>,
    active_tokens: Arc<RwLock<Vec<CancellationToken>>>,
}

impl MonitorManager {
    pub fn new() -> Self {
        Self {
            statuses: Arc::new(RwLock::new(HashMap::new())),
            active_tokens: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    pub async fn get_statuses(&self) -> HashMap<String, LdapStatus> {
        self.statuses.read().await.clone()
    }
    
    pub async fn update_monitors(&self, config: &Config) {
        // Cancel all existing tasks
        {
            let mut tokens = self.active_tokens.write().await;
            for token in tokens.iter() {
                token.cancel();
            }
            tokens.clear();
        }
        
        // Remove statuses of ldaps that are no longer in the config
        {
            let mut statuses_map = self.statuses.write().await;
            let current_ids: std::collections::HashSet<&String> = config.ldaps.iter().map(|l| &l.id).collect();
            statuses_map.retain(|id, _| current_ids.contains(id));
        }
        
        // Spawn new tasks
        let mut tokens = self.active_tokens.write().await;
        for ldap_config in &config.ldaps {
            // Initialize status in the map
            {
                let mut map = self.statuses.write().await;
                map.insert(
                    ldap_config.id.clone(),
                    LdapStatus {
                        id: ldap_config.id.clone(),
                        url: ldap_config.url.clone(),
                        up: false,
                        last_bind_time: None,
                        bind_latency_ms: None,
                        bind_error_message: Some("Initialized, bind check pending".to_string()),
                        bind_success: None,
                        last_search_time: None,
                        search_latency_ms: None,
                        search_error_message: ldap_config.search_check.as_ref().map(|_| "Initialized, search check pending".to_string()),
                        search_success: None,
                    },
                );
            }

            // Bind check loop
            let bind_token = CancellationToken::new();
            tokens.push(bind_token.clone());
            let ldap_config_clone = ldap_config.clone();
            let statuses_clone = self.statuses.clone();
            tokio::spawn(run_bind_loop(ldap_config_clone, statuses_clone, bind_token));

            // Search check loop (only if search check is configured)
            if ldap_config.search_check.is_some() {
                let search_token = CancellationToken::new();
                tokens.push(search_token.clone());
                let ldap_config_clone = ldap_config.clone();
                let statuses_clone = self.statuses.clone();
                tokio::spawn(run_search_loop(ldap_config_clone, statuses_clone, search_token));
            }
        }
        
        tracing::info!("Spawned monitor tasks for {} LDAP targets", config.ldaps.len());
    }
}

async fn run_bind_loop(
    config: LdapTargetConfig,
    statuses: Arc<RwLock<HashMap<String, LdapStatus>>>,
    token: CancellationToken,
) {
    let interval = std::time::Duration::from_secs(config.bind_interval_secs);

    loop {
        let start = std::time::Instant::now();
        let check_result = perform_bind_check(&config).await;
        let duration = start.elapsed().as_millis() as u64;
        
        {
            let mut map = statuses.write().await;
            if let Some(status) = map.get_mut(&config.id) {
                status.last_bind_time = Some(Utc::now());
                status.bind_latency_ms = Some(duration);
                
                match check_result {
                    Ok(()) => {
                        status.bind_success = Some(true);
                        status.bind_error_message = None;
                        tracing::info!(
                            id = %config.id,
                            url = %config.url,
                            latency_ms = duration,
                            "LDAP bind check succeeded"
                        );
                    }
                    Err(err_msg) => {
                        status.bind_success = Some(false);
                        status.bind_error_message = Some(err_msg.clone());
                        tracing::error!(
                            id = %config.id,
                            url = %config.url,
                            error = %err_msg,
                            "LDAP bind check failed"
                        );
                    }
                }
                
                // Update combined status
                let bind_ok = status.bind_success.unwrap_or(false);
                let search_ok = status.search_success.unwrap_or(true);
                status.up = bind_ok && search_ok;
            }
        }
        
        tokio::select! {
            _ = token.cancelled() => {
                tracing::info!("Bind loop for {} cancelled", config.id);
                break;
            }
            _ = tokio::time::sleep(interval) => {}
        }
    }
}

async fn run_search_loop(
    config: LdapTargetConfig,
    statuses: Arc<RwLock<HashMap<String, LdapStatus>>>,
    token: CancellationToken,
) {
    let interval = std::time::Duration::from_secs(config.search_interval_secs);

    loop {
        let start = std::time::Instant::now();
        let check_result = perform_search_check(&config).await;
        let duration = start.elapsed().as_millis() as u64;
        
        {
            let mut map = statuses.write().await;
            if let Some(status) = map.get_mut(&config.id) {
                status.last_search_time = Some(Utc::now());
                status.search_latency_ms = Some(duration);
                
                match check_result {
                    Ok(()) => {
                        status.search_success = Some(true);
                        status.search_error_message = None;
                        tracing::info!(
                            id = %config.id,
                            url = %config.url,
                            latency_ms = duration,
                            "LDAP search check succeeded"
                        );
                    }
                    Err(err_msg) => {
                        status.search_success = Some(false);
                        status.search_error_message = Some(err_msg.clone());
                        tracing::error!(
                            id = %config.id,
                            url = %config.url,
                            error = %err_msg,
                            "LDAP search check failed"
                        );
                    }
                }
                
                // Update combined status
                let bind_ok = status.bind_success.unwrap_or(false);
                let search_ok = status.search_success.unwrap_or(true);
                status.up = bind_ok && search_ok;
            }
        }
        
        tokio::select! {
            _ = token.cancelled() => {
                tracing::info!("Search loop for {} cancelled", config.id);
                break;
            }
            _ = tokio::time::sleep(interval) => {}
        }
    }
}

async fn perform_bind_check(config: &LdapTargetConfig) -> Result<(), String> {
    let timeout_duration = std::time::Duration::from_secs(config.timeout_secs);
    
    tokio::time::timeout(timeout_duration, async {
        let (conn, mut ldap) = LdapConnAsync::new(&config.url)
            .await
            .map_err(|e| format!("Connection error: {}", e))?;
            
        tokio::spawn(async move {
            if let Err(e) = conn.drive().await {
                tracing::debug!("LDAP drive error: {:?}", e);
            }
        });
        
        if let Some(ref bind_dn) = config.bind_dn {
            let password = config.bind_password.as_deref().unwrap_or("");
            ldap.simple_bind(bind_dn, password)
                .await
                .map_err(|e| format!("Bind error: {}", e))?
                .success()
                .map_err(|e| format!("Bind failed: {}", e))?;
        } else {
            // Anonymous bind or just connection check
            ldap.simple_bind("", "")
                .await
                .map_err(|e| format!("Anonymous bind error: {}", e))?
                .success()
                .map_err(|e| format!("Anonymous bind failed: {}", e))?;
        }
        
        let _ = ldap.unbind().await;
        Ok(())
    })
    .await
    .map_err(|_| "Timeout exceeded".to_string())?
}

async fn perform_search_check(config: &LdapTargetConfig) -> Result<(), String> {
    let search = match config.search_check {
        Some(ref s) => s,
        None => return Ok(()),
    };
    
    let timeout_duration = std::time::Duration::from_secs(config.timeout_secs);
    
    tokio::time::timeout(timeout_duration, async {
        let (conn, mut ldap) = LdapConnAsync::new(&config.url)
            .await
            .map_err(|e| format!("Connection error: {}", e))?;
            
        tokio::spawn(async move {
            if let Err(e) = conn.drive().await {
                tracing::debug!("LDAP drive error: {:?}", e);
            }
        });
        
        // Before searching, perform credentials bind
        if let Some(ref bind_dn) = config.bind_dn {
            let password = config.bind_password.as_deref().unwrap_or("");
            ldap.simple_bind(bind_dn, password)
                .await
                .map_err(|e| format!("Bind error before search: {}", e))?
                .success()
                .map_err(|e| format!("Bind failed before search: {}", e))?;
        } else {
            ldap.simple_bind("", "")
                .await
                .map_err(|e| format!("Anonymous bind error before search: {}", e))?
                .success()
                .map_err(|e| format!("Anonymous bind failed before search: {}", e))?;
        }
        
        let search_result = ldap
            .search(
                &search.base,
                search.scope.into(),
                &search.filter,
                vec!["1.1"],
            )
            .await
            .map_err(|e| format!("Search error: {}", e))?;
            
        let (_rs, _res) = search_result
            .success()
            .map_err(|e| format!("Search failed: {}", e))?;
            
        let _ = ldap.unbind().await;
        Ok(())
    })
    .await
    .map_err(|_| "Timeout exceeded".to_string())?
}
