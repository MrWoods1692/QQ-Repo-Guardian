use std::{collections::HashSet, fs, path::Path};

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub bot: BotConfig,
    pub github: GithubConfig,
    pub poller: PollerConfig,
}

impl AppConfig {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: RawAppConfig = toml::from_str(&content)?;
        Ok(config.into())
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RawAppConfig {
    #[serde(default)]
    admins: HashSet<i64>,
    #[serde(default)]
    repositories: Vec<SimpleRepositoryConfig>,
    #[serde(default)]
    server: ServerConfig,
    #[serde(default)]
    bot: BotConfig,
    #[serde(default)]
    poller: PollerConfig,
    #[serde(default)]
    github: RawGithubConfig,
}

impl From<RawAppConfig> for AppConfig {
    fn from(config: RawAppConfig) -> Self {
        let repositories = if config.repositories.is_empty() {
            config.github.repositories
        } else {
            config
                .repositories
                .into_iter()
                .map(RepositoryConfig::from)
                .collect()
        };

        Self {
            server: config.server,
            bot: config.bot,
            poller: config.poller,
            github: GithubConfig {
                webhook_secret: config.github.webhook_secret,
                default_features: config.github.default_features,
                repositories,
                admins: if config.admins.is_empty() {
                    config.github.admins
                } else {
                    config.admins
                },
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawGithubConfig {
    pub webhook_secret: Option<String>,
    #[serde(default)]
    pub default_features: FeatureConfig,
    #[serde(default)]
    pub repositories: Vec<RepositoryConfig>,
    #[serde(default)]
    pub admins: HashSet<i64>,
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
    Napcat {
        #[serde(default = "default_napcat_endpoint")]
        endpoint: String,
        #[serde(default)]
        token: Option<String>,
        #[serde(default = "default_napcat_command")]
        command: Option<String>,
        #[serde(default = "default_napcat_timeout_secs")]
        timeout_secs: u64,
    },
    ProcQq {
        #[serde(default = "default_device_path")]
        device_path: String,
        #[serde(default = "default_session_path")]
        session_path: String,
        #[serde(default = "default_qsign_endpoint")]
        qsign_endpoint: String,
        #[serde(default = "default_qsign_key")]
        qsign_key: String,
        #[serde(default)]
        qsign_command: Option<String>,
        #[serde(default = "default_qsign_timeout_secs")]
        qsign_timeout_secs: u64,
    },
}

impl Default for BotConfig {
    fn default() -> Self {
        Self::Napcat {
            endpoint: default_napcat_endpoint(),
            token: None,
            command: default_napcat_command(),
            timeout_secs: default_napcat_timeout_secs(),
        }
    }
}

fn default_napcat_endpoint() -> String {
    "http://127.0.0.1:3000".to_string()
}

fn default_napcat_command() -> Option<String> {
    Some("./napcat/start.sh".to_string())
}

fn default_napcat_timeout_secs() -> u64 {
    180
}

fn default_device_path() -> String {
    "device.json".to_string()
}

fn default_session_path() -> String {
    "session.token".to_string()
}

fn default_qsign_endpoint() -> String {
    "http://127.0.0.1:8081".to_string()
}

fn default_qsign_key() -> String {
    "114514".to_string()
}

fn default_qsign_timeout_secs() -> u64 {
    900
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
pub struct PollerConfig {
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default = "default_poll_interval_secs")]
    pub interval_secs: u64,
    #[serde(default = "default_poll_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub proxy: Option<String>,
}

impl Default for PollerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: default_poll_interval_secs(),
            timeout_secs: default_poll_timeout_secs(),
            proxy: None,
        }
    }
}

fn default_poll_interval_secs() -> u64 {
    300
}

fn default_poll_timeout_secs() -> u64 {
    15
}

#[derive(Debug, Clone, Deserialize)]
pub struct SimpleRepositoryConfig {
    pub github: String,
    pub repo: String,
    #[serde(default)]
    pub groups: Vec<i64>,
    #[serde(default)]
    pub privates: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepositoryConfig {
    pub full_name: String,
    #[serde(default)]
    pub features: FeatureConfig,
    #[serde(default)]
    pub targets: Vec<NotifyTarget>,
}

impl From<SimpleRepositoryConfig> for RepositoryConfig {
    fn from(config: SimpleRepositoryConfig) -> Self {
        let mut targets = Vec::new();
        targets.extend(
            config
                .groups
                .into_iter()
                .map(|id| NotifyTarget::Group { id }),
        );
        targets.extend(
            config
                .privates
                .into_iter()
                .map(|id| NotifyTarget::Private { id }),
        );

        Self {
            full_name: format!("{}/{}", config.github.trim(), config.repo.trim()),
            features: FeatureConfig::default(),
            targets,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simplified_config() {
        let config: RawAppConfig = toml::from_str(
            r#"
admins = [42]

[[repositories]]
github = "octo"
repo = "repo"
groups = [100]
privates = [42]
"#,
        )
        .unwrap();
        let config = AppConfig::from(config);

        assert!(config.github.admins.contains(&42));
        assert_eq!(config.github.repositories[0].full_name, "octo/repo");
        assert_eq!(
            config.github.repositories[0].targets,
            vec![
                NotifyTarget::Group { id: 100 },
                NotifyTarget::Private { id: 42 }
            ]
        );
        assert!(config.poller.enabled);
    }
}
