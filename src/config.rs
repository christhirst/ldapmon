use serde::{Deserialize, Serialize};

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
