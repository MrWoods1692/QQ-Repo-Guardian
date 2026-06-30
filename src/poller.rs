use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::Context;
use tokio::sync::Mutex;

use crate::{
    config::{GithubConfig, PollerConfig},
    github::{ChangeCard, ChangeCommit, Feature, Notification, build_github_client},
    notifier::Notifier,
};

pub struct GithubPagePoller {
    github: Arc<GithubConfig>,
    notifier: Arc<Notifier>,
    client: reqwest::Client,
    seen_entries: Mutex<HashMap<String, String>>,
}

impl GithubPagePoller {
    pub fn new(
        github: Arc<GithubConfig>,
        notifier: Arc<Notifier>,
        config: &PollerConfig,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            github,
            notifier,
            client: build_github_client(
                config.proxy.as_deref(),
                Duration::from_secs(config.timeout_secs.max(3)),
            )?,
            seen_entries: Mutex::new(HashMap::new()),
        })
    }

    pub async fn run(self: Arc<Self>, interval: Duration) {
        loop {
            if let Err(error) = self.poll_once().await {
                if is_timeout_error(&error) {
                    tracing::warn!(
                        ?error,
                        retry_after_secs = interval.as_secs(),
                        "GitHub page poll timed out; will retry on next interval"
                    );
                } else {
                    tracing::warn!(?error, "GitHub page poll failed");
                }
            }
            tokio::time::sleep(interval).await;
        }
    }

    pub async fn poll_once(&self) -> anyhow::Result<usize> {
        let mut sent = 0;
        for repository in &self.github.repositories {
            let Some(entry) = self
                .fetch_latest_commit(&repository.full_name)
                .await
                .with_context(|| {
                    format!("failed to fetch {} commits page", repository.full_name)
                })?
            else {
                continue;
            };

            let mut seen_entries = self.seen_entries.lock().await;
            let previous = seen_entries.insert(repository.full_name.clone(), entry.id.clone());
            drop(seen_entries);

            if previous.is_none() || previous.as_deref() == Some(entry.id.as_str()) {
                continue;
            }

            sent += self
                .notifier
                .dispatch(
                    &self.github,
                    Notification {
                        repository: repository.full_name.clone(),
                        feature: Feature::Pushes,
                        message: format!(
                            "仓库网页检测到新提交\n仓库: {}\n提交: {}\n作者: {}\n{}",
                            repository.full_name, entry.title, entry.author, entry.link
                        ),
                        card: Some(change_card_from_feed_entry(&repository.full_name, &entry)),
                    },
                )
                .await?;
        }

        Ok(sent)
    }

    async fn fetch_latest_commit(&self, repository: &str) -> anyhow::Result<Option<FeedEntry>> {
        let url = format!("https://github.com/{repository}/commits.atom");
        let body = self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        Ok(parse_latest_commit_entry(&body))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FeedEntry {
    id: String,
    title: String,
    author: String,
    link: String,
}

fn parse_latest_commit_entry(feed: &str) -> Option<FeedEntry> {
    let entry = between(feed, "<entry>", "</entry>")?;
    Some(FeedEntry {
        id: xml_text(entry, "id")?,
        title: xml_text(entry, "title").unwrap_or_else(|| "unknown".to_string()),
        author: between(entry, "<author>", "</author>")
            .and_then(|author| xml_text(author, "name"))
            .unwrap_or_else(|| "unknown".to_string()),
        link: entry
            .split("<link")
            .nth(1)
            .and_then(|link| between(link, "href=\"", "\""))
            .map(html_unescape)
            .unwrap_or_default(),
    })
}

fn change_card_from_feed_entry(repository: &str, entry: &FeedEntry) -> ChangeCard {
    ChangeCard {
        title: "仓库网页检测到新提交".to_string(),
        repository: repository.to_string(),
        branch: "默认分支".to_string(),
        actor: entry.author.clone(),
        summary: "新增 1 个提交".to_string(),
        url: entry.link.clone(),
        commits: vec![ChangeCommit {
            message: entry.title.clone(),
            author: entry.author.clone(),
            url: entry.link.clone(),
        }],
    }
}

fn xml_text(source: &str, tag: &str) -> Option<String> {
    between(source, &format!("<{tag}>"), &format!("</{tag}>"))
        .map(html_unescape)
        .map(|value| value.trim().to_string())
}

fn between<'a>(source: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let value = source.split_once(start)?.1;
    Some(value.split_once(end)?.0)
}

fn is_timeout_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<reqwest::Error>()
            .is_some_and(reqwest::Error::is_timeout)
    })
}

fn html_unescape(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_latest_commit_entry_from_atom_feed() {
        let feed = r#"<feed><entry><id>tag:github.com,2008:Grit::Commit/abc</id><title>fix &amp; ship</title><author><name>alice</name></author><link rel="alternate" type="text/html" href="https://github.com/octo/repo/commit/abc" /></entry></feed>"#;

        let entry = parse_latest_commit_entry(feed).unwrap();

        assert_eq!(entry.id, "tag:github.com,2008:Grit::Commit/abc");
        assert_eq!(entry.title, "fix & ship");
        assert_eq!(entry.author, "alice");
        assert_eq!(entry.link, "https://github.com/octo/repo/commit/abc");
    }

    #[test]
    fn page_polling_card_uses_readable_default_branch_label() {
        let entry = FeedEntry {
            id: "tag:github.com,2008:Grit::Commit/abc".to_string(),
            title: "fix card".to_string(),
            author: "alice".to_string(),
            link: "https://github.com/octo/repo/commit/abc".to_string(),
        };

        let card = change_card_from_feed_entry("octo/repo", &entry);

        assert_eq!(card.branch, "默认分支");
        assert_eq!(card.summary, "新增 1 个提交");
    }
}
