use std::{collections::HashSet, fs, path::Path};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,
    pub bot: BotConfig,
    pub github: GithubConfig,
}

impl AppConfig {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
        }
    }
}

fn default_bind() -> String {
    "127.0.0.1:8080".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BotConfig {
    Mock,
    OneBot {
        endpoint: String,
        access_token: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct GithubConfig {
    pub webhook_secret: Option<String>,
    #[serde(default)]
    pub default_features: FeatureConfig,
    #[serde(default)]
    pub repositories: Vec<RepositoryConfig>,
    #[serde(default)]
    pub admins: HashSet<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepositoryConfig {
    pub full_name: String,
    #[serde(default)]
    pub features: FeatureConfig,
    #[serde(default)]
    pub targets: Vec<NotifyTarget>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotifyTarget {
    Group { id: i64 },
    Private { id: i64 },
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeatureConfig {
    #[serde(default = "enabled")]
    pub issues: bool,
    #[serde(default = "enabled")]
    pub pull_requests: bool,
    #[serde(default = "enabled")]
    pub pushes: bool,
    #[serde(default = "enabled")]
    pub checks: bool,
    #[serde(default = "enabled")]
    pub contributors: bool,
    #[serde(default = "enabled")]
    pub releases: bool,
    #[serde(default = "enabled")]
    pub stars: bool,
    #[serde(default = "enabled")]
    pub forks: bool,
}

impl Default for FeatureConfig {
    fn default() -> Self {
        Self {
            issues: true,
            pull_requests: true,
            pushes: true,
            checks: true,
            contributors: true,
            releases: true,
            stars: true,
            forks: true,
        }
    }
}

fn enabled() -> bool {
    true
}
