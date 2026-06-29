use std::sync::Arc;

use crate::{
    bot::BotClient,
    config::{GithubConfig, NotifyTarget},
    github::Notification,
};

pub struct Notifier {
    bot: Arc<dyn BotClient>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bot::MockBot,
        config::{FeatureConfig, RepositoryConfig, SimpleRepositoryConfig},
        github::Feature,
    };

    fn config_for(repository: RepositoryConfig) -> GithubConfig {
        GithubConfig {
            webhook_secret: None,
            default_features: FeatureConfig::default(),
            repositories: vec![repository],
            admins: Default::default(),
        }
    }

    #[tokio::test]
    async fn dispatch_sends_to_all_repository_targets() {
        let bot = Arc::new(MockBot::default());
        let notifier = Notifier::new(bot.clone());
        let config = config_for(RepositoryConfig::from(SimpleRepositoryConfig {
            github: "octo".to_string(),
            repo: "repo".to_string(),
            groups: vec![100],
            privates: vec![42],
        }));

        let sent = notifier
            .dispatch(
                &config,
                Notification {
                    repository: "octo/repo".to_string(),
                    feature: Feature::Issues,
                    message: "hello".to_string(),
                },
            )
            .await
            .unwrap();

        assert_eq!(sent, 2);
        assert_eq!(bot.messages().len(), 2);
    }

    #[tokio::test]
    async fn dispatch_skips_disabled_repository_feature() {
        let bot = Arc::new(MockBot::default());
        let notifier = Notifier::new(bot.clone());
        let mut features = FeatureConfig::default();
        features.issues = false;
        let mut repository = RepositoryConfig::from(SimpleRepositoryConfig {
            github: "octo".to_string(),
            repo: "repo".to_string(),
            groups: vec![100],
            privates: vec![],
        });
        repository.features = features;
        let config = config_for(repository);

        let sent = notifier
            .dispatch(
                &config,
                Notification {
                    repository: "octo/repo".to_string(),
                    feature: Feature::Issues,
                    message: "hello".to_string(),
                },
            )
            .await
            .unwrap();

        assert_eq!(sent, 0);
        assert!(bot.messages().is_empty());
    }
}

impl Notifier {
    pub fn new(bot: Arc<dyn BotClient>) -> Self {
        Self { bot }
    }

    pub async fn dispatch(
        &self,
        config: &GithubConfig,
        notification: Notification,
    ) -> anyhow::Result<usize> {
        let Some(repository) = config
            .repositories
            .iter()
            .find(|repo| repo.full_name == notification.repository)
        else {
            tracing::debug!(repository = %notification.repository, "repository is not configured");
            return Ok(0);
        };

        if !notification
            .feature
            .enabled(&repository.features, &config.default_features)
        {
            tracing::debug!(repository = %notification.repository, feature = ?notification.feature, "notification feature disabled");
            return Ok(0);
        }

        for target in &repository.targets {
            self.send_direct(target, &notification.message).await?;
        }

        Ok(repository.targets.len())
    }

    pub async fn send_direct(&self, target: &NotifyTarget, message: &str) -> anyhow::Result<()> {
        self.bot.send(target, message).await
    }
}
