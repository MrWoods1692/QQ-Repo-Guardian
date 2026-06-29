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
    let slug = parse_github_repository_url(url)?;

    Some(format!(
        "GitHub 仓库卡片\n仓库: {}\n链接: {}",
        slug.full_name, slug.url
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositorySlug {
    pub owner: String,
    pub repo: String,
    pub full_name: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryCard {
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub url: String,
    pub avatar_url: String,
    pub about: String,
    pub stars: u64,
    pub forks: u64,
    pub issues: u64,
    pub recent_commit_time: String,
}

#[derive(Debug, Deserialize)]
struct ApiRepository {
    name: String,
    full_name: String,
    html_url: String,
    description: Option<String>,
    stargazers_count: u64,
    forks_count: u64,
    open_issues_count: u64,
    pushed_at: Option<String>,
    owner: ApiOwner,
}

#[derive(Debug, Deserialize)]
struct ApiOwner {
    login: String,
    avatar_url: String,
}

pub fn parse_github_repository_url(url: &str) -> Option<RepositorySlug> {
    let parsed = url::Url::parse(url).ok()?;
    if parsed.host_str()? != "github.com" {
        return None;
    }

    let mut segments = parsed.path_segments()?;
    let owner = segments.next()?.trim().to_string();
    let repo = segments.next()?.trim_end_matches(".git").to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }

    let full_name = format!("{owner}/{repo}");
    Some(RepositorySlug {
        owner,
        repo,
        full_name: full_name.clone(),
        url: format!("https://github.com/{full_name}"),
    })
}

pub async fn fetch_repo_card(url: &str) -> anyhow::Result<RepositoryCard> {
    let slug = parse_github_repository_url(url)
        .ok_or_else(|| anyhow::anyhow!("not a github repository url"))?;
    let api_url = format!("https://api.github.com/repos/{}/{}", slug.owner, slug.repo);
    let body = reqwest::Client::new()
        .get(api_url)
        .header(reqwest::header::USER_AGENT, "qq-repo-guardian")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let repository: ApiRepository = serde_json::from_str(&body)?;

    Ok(RepositoryCard {
        owner: repository.owner.login,
        name: repository.name,
        full_name: repository.full_name,
        url: repository.html_url,
        avatar_url: repository.owner.avatar_url,
        about: repository
            .description
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "No description provided.".to_string()),
        stars: repository.stargazers_count,
        forks: repository.forks_count,
        issues: repository.open_issues_count,
        recent_commit_time: repository
            .pushed_at
            .map(format_github_time)
            .unwrap_or_else(|| "unknown".to_string()),
    })
}

pub fn render_repo_card_html(card: &RepositoryCard) -> String {
    let svg = render_repo_card_svg(card);
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{}</title>
  <style>
    html, body {{ margin: 0; min-height: 100%; background: #eef2ff; }}
    body {{ display: grid; place-items: center; padding: 32px; box-sizing: border-box; }}
    img {{ width: min(760px, 100%); height: auto; display: block; filter: drop-shadow(0 22px 48px rgba(15, 23, 42, .22)); }}
  </style>
</head>
<body>
  <img alt="{}" src="data:image/svg+xml;charset=utf-8,{}">
</body>
</html>"#,
        escape_html(&card.full_name),
        escape_html(&card.full_name),
        url::form_urlencoded::byte_serialize(svg.as_bytes()).collect::<String>()
    )
}

pub fn render_repo_card_svg(card: &RepositoryCard) -> String {
    let about = truncate_for_card(&card.about, 120);
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="760" height="420" viewBox="0 0 760 420">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#0ea5e9"/>
      <stop offset="0.46" stop-color="#4f46e5"/>
      <stop offset="1" stop-color="#ec4899"/>
    </linearGradient>
    <linearGradient id="panel" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#ffffff" stop-opacity="0.96"/>
      <stop offset="1" stop-color="#f8fafc" stop-opacity="0.92"/>
    </linearGradient>
    <clipPath id="avatarClip"><circle cx="96" cy="96" r="46"/></clipPath>
    <filter id="shadow" x="-10%" y="-10%" width="120%" height="130%">
      <feDropShadow dx="0" dy="16" stdDeviation="18" flood-color="#0f172a" flood-opacity="0.25"/>
    </filter>
  </defs>
  <rect width="760" height="420" rx="28" fill="url(#bg)"/>
  <circle cx="630" cy="74" r="92" fill="#ffffff" opacity="0.13"/>
  <circle cx="692" cy="334" r="138" fill="#fef3c7" opacity="0.18"/>
  <rect x="36" y="36" width="688" height="348" rx="22" fill="url(#panel)" filter="url(#shadow)"/>
  <image x="50" y="50" width="92" height="92" href="{}" preserveAspectRatio="xMidYMid slice" clip-path="url(#avatarClip)"/>
  <circle cx="96" cy="96" r="47" fill="none" stroke="#ffffff" stroke-width="4"/>
  <text x="164" y="78" fill="#64748b" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="21" font-weight="700">{}</text>
  <text x="164" y="118" fill="#0f172a" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="34" font-weight="800">{}</text>
  <text x="164" y="150" fill="#2563eb" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="18">{}</text>
  <text x="54" y="214" fill="#334155" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="20" font-weight="650">About</text>
  <text x="54" y="246" fill="#475569" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="19">{}</text>
  <rect x="54" y="286" width="146" height="64" rx="16" fill="#eff6ff"/>
  <rect x="218" y="286" width="146" height="64" rx="16" fill="#f0fdf4"/>
  <rect x="382" y="286" width="146" height="64" rx="16" fill="#fff7ed"/>
  <rect x="546" y="286" width="132" height="64" rx="16" fill="#fdf2f8"/>
  <text x="76" y="314" fill="#1d4ed8" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="17" font-weight="700">Stars</text>
  <text x="76" y="340" fill="#0f172a" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="25" font-weight="800">{}</text>
  <text x="240" y="314" fill="#15803d" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="17" font-weight="700">Forks</text>
  <text x="240" y="340" fill="#0f172a" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="25" font-weight="800">{}</text>
  <text x="404" y="314" fill="#c2410c" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="17" font-weight="700">Issues</text>
  <text x="404" y="340" fill="#0f172a" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="25" font-weight="800">{}</text>
  <text x="568" y="314" fill="#be185d" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="17" font-weight="700">Recent</text>
  <text x="568" y="340" fill="#0f172a" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="19" font-weight="800">{}</text>
</svg>"##,
        escape_html(&card.avatar_url),
        escape_html(&card.owner),
        escape_html(&card.name),
        escape_html(&card.url),
        escape_html(&about),
        format_count(card.stars),
        format_count(card.forks),
        format_count(card.issues),
        escape_html(&card.recent_commit_time),
    )
}

fn format_count(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

fn format_github_time(value: String) -> String {
    value
        .strip_suffix('Z')
        .unwrap_or(&value)
        .split_once('T')
        .map(|(date, _)| date.to_string())
        .unwrap_or(value)
}

fn truncate_for_card(value: &str, max_chars: usize) -> String {
    let mut result = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        result.push('…');
    }
    result
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
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

    #[test]
    fn renders_colorful_repo_card_svg() {
        let card = RepositoryCard {
            owner: "owner".to_string(),
            name: "project".to_string(),
            full_name: "owner/project".to_string(),
            url: "https://github.com/owner/project".to_string(),
            avatar_url: "https://avatars.githubusercontent.com/u/1?v=4".to_string(),
            about: "A useful project".to_string(),
            stars: 15320,
            forks: 821,
            issues: 12,
            recent_commit_time: "2026-06-29".to_string(),
        };
        let svg = render_repo_card_svg(&card);

        assert!(svg.contains("owner"));
        assert!(svg.contains("project"));
        assert!(svg.contains("15.3K"));
        assert!(svg.contains("Issues"));
        assert!(svg.contains("A useful project"));
    }
}
