use std::sync::Arc;

use crate::{
    bot::BotClient,
    config::{GithubConfig, NotifyTarget},
    github::{self, Notification},
};

pub struct Notifier {
    bot: Arc<dyn BotClient>,
    public_base_url: Option<Arc<str>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bot::MockBot,
        config::{FeatureConfig, RepositoryConfig, SimpleRepositoryConfig},
        github::{ChangeCard, ChangeCommit, Feature},
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
                    card: None,
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
                    card: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(sent, 0);
        assert!(bot.messages().is_empty());
    }

    #[tokio::test]
    async fn dispatch_renders_change_card_image_when_available() {
        let bot = Arc::new(MockBot::default());
        let notifier =
            Notifier::with_public_base_url(bot.clone(), "http://127.0.0.1:8080/".to_string());
        let config = config_for(RepositoryConfig::from(SimpleRepositoryConfig {
            github: "octo".to_string(),
            repo: "repo".to_string(),
            groups: vec![100],
            privates: vec![],
        }));

        notifier
            .dispatch(
                &config,
                Notification {
                    repository: "octo/repo".to_string(),
                    feature: Feature::Pushes,
                    message: "fallback".to_string(),
                    card: Some(ChangeCard {
                        title: "新的代码提交".to_string(),
                        repository: "octo/repo".to_string(),
                        branch: "main".to_string(),
                        actor: "alice".to_string(),
                        summary: "2 commits pushed".to_string(),
                        url: "https://github.com/octo/repo/compare/a...b".to_string(),
                        commits: vec![ChangeCommit {
                            message: "fix card".to_string(),
                            author: "alice".to_string(),
                            url: "https://github.com/octo/repo/commit/b".to_string(),
                        }],
                    }),
                },
            )
            .await
            .unwrap();

        let messages = bot.messages();
        assert_eq!(messages.len(), 1);
        assert!(
            messages[0]
                .1
                .starts_with("[CQ:image,file=http://127.0.0.1:8080/github/change.png?")
        );
        assert!(messages[0].1.contains("fallback"));
    }
}

impl Notifier {
    pub fn new(bot: Arc<dyn BotClient>) -> Self {
        Self {
            bot,
            public_base_url: None,
        }
    }

    pub fn with_public_base_url(bot: Arc<dyn BotClient>, public_base_url: String) -> Self {
        Self {
            bot,
            public_base_url: Some(public_base_url.into()),
        }
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

        let message = self.render_message(&notification);
        for target in &repository.targets {
            self.send_direct(target, &message).await?;
        }

        Ok(repository.targets.len())
    }

    pub async fn send_direct(&self, target: &NotifyTarget, message: &str) -> anyhow::Result<()> {
        self.bot.send(target, message).await
    }

    pub async fn sign_group(&self, group_id: i64) -> anyhow::Result<()> {
        self.bot.sign_group(group_id).await
    }

    fn render_message(&self, notification: &Notification) -> String {
        if let (Some(base_url), Some(card)) = (&self.public_base_url, &notification.card) {
            return format!(
                "[CQ:image,file={}/github/change.png?{}]\n{}",
                base_url.trim_end_matches('/'),
                github::change_card_query(card),
                notification.message
            );
        }

        notification.message.clone()
    }
}
