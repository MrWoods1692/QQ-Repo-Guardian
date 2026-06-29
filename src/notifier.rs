use std::sync::Arc;

use crate::{
    bot::BotClient,
    config::{GithubConfig, NotifyTarget},
    github::Notification,
};

pub struct Notifier {
    bot: Arc<dyn BotClient>,
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
