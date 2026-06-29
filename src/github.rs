use serde::Deserialize;
use serde_json::Value;

use crate::config::FeatureConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub repository: String,
    pub feature: Feature,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    Issues,
    PullRequests,
    Pushes,
    Checks,
    Contributors,
    Releases,
    Stars,
    Forks,
}

impl Feature {
    pub fn enabled(self, local: &FeatureConfig, default: &FeatureConfig) -> bool {
        match self {
            Feature::Issues => local.issues && default.issues,
            Feature::PullRequests => local.pull_requests && default.pull_requests,
            Feature::Pushes => local.pushes && default.pushes,
            Feature::Checks => local.checks && default.checks,
            Feature::Contributors => local.contributors && default.contributors,
            Feature::Releases => local.releases && default.releases,
            Feature::Stars => local.stars && default.stars,
            Feature::Forks => local.forks && default.forks,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Repository {
    full_name: String,
    html_url: Option<String>,
    stargazers_count: Option<u64>,
    forks_count: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct Sender {
    login: String,
}

pub fn parse_event(event: &str, payload: &[u8]) -> anyhow::Result<Option<Notification>> {
    let value: Value = serde_json::from_slice(payload)?;
    let repository: Repository = serde_json::from_value(value["repository"].clone())?;
    let actor = serde_json::from_value::<Sender>(value["sender"].clone())
        .map(|sender| sender.login)
        .unwrap_or_else(|_| "unknown".to_string());
    let repo_url = repository
        .html_url
        .clone()
        .unwrap_or_else(|| format!("https://github.com/{}", repository.full_name));

    let notification = match event {
        "issues" => issue_notification(&value, &repository, &actor),
        "pull_request" => pull_request_notification(&value, &repository, &actor),
        "push" => push_notification(&value, &repository, &actor, &repo_url),
        "check_run" | "check_suite" => check_notification(&value, &repository),
        "member" => member_notification(&value, &repository),
        "release" => release_notification(&value, &repository, &actor),
        "star" => star_notification(&value, &repository, &actor, &repo_url),
        "fork" => fork_notification(&value, &repository, &actor),
        _ => return Ok(None),
    };

    Ok(Some(notification))
}

pub fn render_repo_card(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    if parsed.host_str()? != "github.com" {
        return None;
    }

    let mut segments = parsed.path_segments()?;
    let owner = segments.next()?;
    let repo = segments.next()?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }

    Some(format!(
        "GitHub 仓库卡片\n仓库: {owner}/{repo}\n链接: https://github.com/{owner}/{repo}"
    ))
}

fn issue_notification(value: &Value, repository: &Repository, actor: &str) -> Notification {
    let action = value["action"].as_str().unwrap_or("updated");
    let issue = &value["issue"];
    let number = issue["number"].as_u64().unwrap_or_default();
    let title = issue["title"].as_str().unwrap_or("untitled");
    let url = issue["html_url"].as_str().unwrap_or_default();
    let message = match action {
        "opened" => format!(
            "新 Issue #{}: {}\n提交者: {}\n{}",
            number, title, actor, url
        ),
        "closed" => format!(
            "Issue 已解决 #{}: {}\n处理者: {}\n{}",
            number, title, actor, url
        ),
        _ => format!(
            "Issue 更新 #{}: {}\n动作: {}\n{}",
            number, title, action, url
        ),
    };
    Notification {
        repository: repository.full_name.clone(),
        feature: Feature::Issues,
        message,
    }
}

fn pull_request_notification(value: &Value, repository: &Repository, actor: &str) -> Notification {
    let action = value["action"].as_str().unwrap_or("updated");
    let pr = &value["pull_request"];
    let number = pr["number"].as_u64().unwrap_or_default();
    let title = pr["title"].as_str().unwrap_or("untitled");
    let url = pr["html_url"].as_str().unwrap_or_default();
    let merged = pr["merged"].as_bool().unwrap_or(false);
    let message = match (action, merged) {
        ("opened", _) => format!("新 PR #{}: {}\n提交者: {}\n{}", number, title, actor, url),
        ("closed", true) => format!(
            "PR 已合并 #{}: {}\n合并者: {}\n{}",
            number, title, actor, url
        ),
        ("closed", false) => format!(
            "PR 已关闭/拒绝 #{}: {}\n处理者: {}\n{}",
            number, title, actor, url
        ),
        _ => format!("PR 更新 #{}: {}\n动作: {}\n{}", number, title, action, url),
    };
    Notification {
        repository: repository.full_name.clone(),
        feature: Feature::PullRequests,
        message,
    }
}

fn push_notification(
    value: &Value,
    repository: &Repository,
    actor: &str,
    repo_url: &str,
) -> Notification {
    let branch = value["ref"]
        .as_str()
        .unwrap_or("refs/heads/unknown")
        .trim_start_matches("refs/heads/");
    let count = value["commits"].as_array().map_or(0, Vec::len);
    let compare = value["compare"].as_str().unwrap_or(repo_url);
    let message = format!(
        "新的代码提交\n仓库: {}\n分支: {}\n提交数: {}\n提交者: {}\n{}",
        repository.full_name, branch, count, actor, compare
    );
    Notification {
        repository: repository.full_name.clone(),
        feature: Feature::Pushes,
        message,
    }
}

fn check_notification(value: &Value, repository: &Repository) -> Notification {
    let item = value
        .get("check_run")
        .or_else(|| value.get("check_suite"))
        .unwrap_or(value);
    let name = item["name"].as_str().unwrap_or("checks");
    let conclusion = item["conclusion"]
        .as_str()
        .or_else(|| item["status"].as_str())
        .unwrap_or("unknown");
    let url = item["html_url"].as_str().unwrap_or_default();
    let message = format!(
        "提交检查更新\n仓库: {}\n检查: {}\n状态: {}\n{}",
        repository.full_name, name, conclusion, url
    );
    Notification {
        repository: repository.full_name.clone(),
        feature: Feature::Checks,
        message,
    }
}

fn member_notification(value: &Value, repository: &Repository) -> Notification {
    let action = value["action"].as_str().unwrap_or("updated");
    let member = value["member"]["login"].as_str().unwrap_or("unknown");
    let message = if action == "added" {
        format!(
            "新的贡献者\n仓库: {}\n成员: {}",
            repository.full_name, member
        )
    } else {
        format!(
            "贡献者更新\n仓库: {}\n动作: {}",
            repository.full_name, action
        )
    };
    Notification {
        repository: repository.full_name.clone(),
        feature: Feature::Contributors,
        message,
    }
}

fn release_notification(value: &Value, repository: &Repository, actor: &str) -> Notification {
    let action = value["action"].as_str().unwrap_or("updated");
    let release = &value["release"];
    let name = release["name"]
        .as_str()
        .or_else(|| release["tag_name"].as_str())
        .unwrap_or("untitled");
    let url = release["html_url"].as_str().unwrap_or_default();
    let message = format!(
        "Release {}\n仓库: {}\n版本: {}\n发布者: {}\n{}",
        action, repository.full_name, name, actor, url
    );
    Notification {
        repository: repository.full_name.clone(),
        feature: Feature::Releases,
        message,
    }
}

fn star_notification(
    value: &Value,
    repository: &Repository,
    actor: &str,
    repo_url: &str,
) -> Notification {
    let action = value["action"].as_str().unwrap_or("created");
    let count = repository.stargazers_count.unwrap_or_default();
    let message = if action == "created" {
        format!(
            "Star 数量增加\n仓库: {}\n当前 Star: {}\n来自: {}\n{}",
            repository.full_name, count, actor, repo_url
        )
    } else {
        format!(
            "Star 更新\n仓库: {}\n动作: {}",
            repository.full_name, action
        )
    };
    Notification {
        repository: repository.full_name.clone(),
        feature: Feature::Stars,
        message,
    }
}

fn fork_notification(value: &Value, repository: &Repository, actor: &str) -> Notification {
    let fork_url = value["forkee"]["html_url"].as_str().unwrap_or_default();
    let count = repository.forks_count.unwrap_or_default();
    Notification {
        repository: repository.full_name.clone(),
        feature: Feature::Forks,
        message: format!(
            "Fork 数量增加\n仓库: {}\n当前 Fork: {}\n来自: {}\n{}",
            repository.full_name, count, actor, fork_url
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_open_issue() {
        let payload = br#"{"action":"opened","repository":{"full_name":"octo/repo","html_url":"https://github.com/octo/repo"},"issue":{"number":7,"title":"Bug","html_url":"https://github.com/octo/repo/issues/7"},"sender":{"login":"alice"}}"#;
        let notification = parse_event("issues", payload).unwrap().unwrap();
        assert_eq!(notification.repository, "octo/repo");
        assert_eq!(notification.feature, Feature::Issues);
        assert!(notification.message.contains("新 Issue #7"));
    }

    #[test]
    fn renders_repo_card_from_github_url() {
        let card = render_repo_card("https://github.com/owner/project/issues/1").unwrap();
        assert!(card.contains("owner/project"));
    }
}
