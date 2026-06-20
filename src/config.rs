use anyhow::Context;
use serde::{Deserialize, Serialize};

impl Config {
    /// Load configuration using config-rs.
    /// - `None`  → auto-discovers `config.{yaml,yml,json,toml,…}` in the current directory.
    /// - `Some(path)` → loads exactly that file; format is detected from the extension.
    pub fn load(path: Option<&str>) -> Result<Self, anyhow::Error> {
        let mut builder = ::config::Config::builder();
        builder = match path {
            Some(p) => builder.add_source(::config::File::from(std::path::Path::new(p))),
            None    => builder.add_source(::config::File::with_name("config")),
        };
        builder
            .build()
            .with_context(|| match path {
                Some(p) => format!("Failed to read config file '{}'", p),
                None    => "Failed to read config file (looked for config.yaml/json/toml/…)".into(),
            })?
            .try_deserialize::<Self>()
            .context("Failed to parse configuration")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub bind_address: String,
    pub ldaps: Vec<LdapTargetConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LdapTargetConfig {
    pub id: String,
    pub url: String,
    pub bind_dn: Option<String>,
    pub bind_password: Option<String>,

    #[serde(default = "default_check_interval")]
    pub bind_interval_secs: u64,

    #[serde(default = "default_check_interval")]
    pub search_interval_secs: u64,

    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    pub search_check: Option<SearchCheckConfig>,
}

fn default_check_interval() -> u64 {
    10
}

fn default_timeout() -> u64 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchCheckConfig {
    pub base: String,
    #[serde(default = "default_filter")]
    pub filter: String,
    #[serde(default = "default_scope")]
    pub scope: SearchScope,
}

fn default_filter() -> String {
    "(objectClass=*)".to_string()
}

fn default_scope() -> SearchScope {
    SearchScope::Subtree
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SearchScope {
    Base,
    OneLevel,
    Subtree,
}

impl From<SearchScope> for ldap3::Scope {
    fn from(s: SearchScope) -> Self {
        match s {
            SearchScope::Base => ldap3::Scope::Base,
            SearchScope::OneLevel => ldap3::Scope::OneLevel,
            SearchScope::Subtree => ldap3::Scope::Subtree,
        }
    }
}
