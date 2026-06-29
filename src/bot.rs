use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde::Serialize;

use crate::config::{BotConfig, NotifyTarget};

#[async_trait]
pub trait BotClient: Send + Sync {
    async fn send(&self, target: &NotifyTarget, message: &str) -> anyhow::Result<()>;
}

pub fn from_config(config: &BotConfig) -> anyhow::Result<Arc<dyn BotClient>> {
    match config {
        BotConfig::Mock => Ok(Arc::new(MockBot::default())),
        BotConfig::OneBot {
            endpoint,
            access_token,
        } => Ok(Arc::new(OneBotClient::new(
            endpoint,
            access_token.as_deref(),
        )?)),
    }
}

#[derive(Default)]
pub struct MockBot {
    messages: Mutex<Vec<(NotifyTarget, String)>>,
}

impl MockBot {
    pub fn messages(&self) -> Vec<(NotifyTarget, String)> {
        self.messages
            .lock()
            .expect("mock bot mutex poisoned")
            .clone()
    }
}

#[async_trait]
impl BotClient for MockBot {
    async fn send(&self, target: &NotifyTarget, message: &str) -> anyhow::Result<()> {
        self.messages
            .lock()
            .expect("mock bot mutex poisoned")
            .push((target.clone(), message.to_string()));
        tracing::info!(?target, %message, "mock bot message");
        Ok(())
    }
}

pub struct OneBotClient {
    endpoint: String,
    client: reqwest::Client,
}

impl OneBotClient {
    pub fn new(endpoint: &str, access_token: Option<&str>) -> anyhow::Result<Self> {
        let mut headers = HeaderMap::new();
        if let Some(token) = access_token {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}"))?,
            );
        }

        Ok(Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            client: reqwest::Client::builder()
                .default_headers(headers)
                .build()?,
        })
    }
}

#[derive(Serialize)]
struct GroupMessage<'a> {
    group_id: i64,
    message: &'a str,
}

#[derive(Serialize)]
struct PrivateMessage<'a> {
    user_id: i64,
    message: &'a str,
}

#[async_trait]
impl BotClient for OneBotClient {
    async fn send(&self, target: &NotifyTarget, message: &str) -> anyhow::Result<()> {
        match target {
            NotifyTarget::Group { id } => {
                self.client
                    .post(format!("{}/send_group_msg", self.endpoint))
                    .json(&GroupMessage {
                        group_id: *id,
                        message,
                    })
                    .send()
                    .await?
                    .error_for_status()?;
            }
            NotifyTarget::Private { id } => {
                self.client
                    .post(format!("{}/send_private_msg", self.endpoint))
                    .json(&PrivateMessage {
                        user_id: *id,
                        message,
                    })
                    .send()
                    .await?
                    .error_for_status()?;
            }
        }
        Ok(())
    }
}
