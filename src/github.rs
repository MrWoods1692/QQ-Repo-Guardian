use base64::Engine;
use serde::Deserialize;
use serde_json::Value;

use crate::config::FeatureConfig;

pub fn build_github_client(
    proxy: Option<&str>,
    timeout: std::time::Duration,
) -> anyhow::Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .user_agent("qq-repo-guardian")
        .timeout(timeout);

    if let Some(proxy) = proxy.filter(|value| !value.trim().is_empty()) {
        builder = builder.proxy(reqwest::Proxy::all(proxy)?);
    }

    Ok(builder.build()?)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub repository: String,
    pub feature: Feature,
    pub message: String,
    pub card: Option<ChangeCard>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeCard {
    pub title: String,
    pub repository: String,
    pub branch: String,
    pub actor: String,
    pub summary: String,
    pub url: String,
    pub commits: Vec<ChangeCommit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeCommit {
    pub message: String,
    pub author: String,
    pub url: String,
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
    pub avatar_data_uri: Option<String>,
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

pub async fn fetch_repo_card(
    client: &reqwest::Client,
    url: &str,
) -> anyhow::Result<RepositoryCard> {
    let slug = parse_github_repository_url(url)
        .ok_or_else(|| anyhow::anyhow!("not a github repository url"))?;
    let api_url = format!("https://api.github.com/repos/{}/{}", slug.owner, slug.repo);
    let body = client
        .get(api_url)
        .header(reqwest::header::USER_AGENT, "qq-repo-guardian")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let repository: ApiRepository = serde_json::from_str(&body)?;
    let avatar_data_uri = fetch_avatar_data_uri(&client, &repository.owner.avatar_url).await;

    Ok(RepositoryCard {
        owner: repository.owner.login,
        name: repository.name,
        full_name: repository.full_name,
        url: repository.html_url,
        avatar_url: repository.owner.avatar_url,
        avatar_data_uri,
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

async fn fetch_avatar_data_uri(client: &reqwest::Client, url: &str) -> Option<String> {
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "qq-repo-guardian")
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("image/png")
        .split(';')
        .next()
        .unwrap_or("image/png")
        .to_string();
    if !content_type.starts_with("image/") {
        return None;
    }
    let bytes = response.bytes().await.ok()?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{content_type};base64,{encoded}"))
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
    let avatar_href = card.avatar_data_uri.as_deref().unwrap_or(&card.avatar_url);
    let about_lines = svg_text_lines(&card.about, 48, 2, 58, 238, 21, 0);
    let recent = truncate_for_card(&card.recent_commit_time, 10);
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="760" height="420" viewBox="0 0 760 420">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0" stop-color="#0f766e"/>
            <stop offset="0.48" stop-color="#2563eb"/>
            <stop offset="1" stop-color="#db2777"/>
    </linearGradient>
    <linearGradient id="panel" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0" stop-color="#ffffff" stop-opacity="0.98"/>
            <stop offset="1" stop-color="#f8fafc" stop-opacity="0.95"/>
    </linearGradient>
        <clipPath id="avatarClip"><circle cx="102" cy="102" r="48"/></clipPath>
        <clipPath id="aboutClip"><rect x="58" y="218" width="632" height="48" rx="0"/></clipPath>
    <filter id="shadow" x="-10%" y="-10%" width="120%" height="130%">
            <feDropShadow dx="0" dy="18" stdDeviation="20" flood-color="#0f172a" flood-opacity="0.28"/>
    </filter>
  </defs>
  <rect width="760" height="420" rx="28" fill="url(#bg)"/>
    <path d="M0 104 C90 44 166 40 260 88 C348 132 420 100 514 48 C606 -2 698 16 760 76 L760 0 L0 0Z" fill="#ffffff" opacity="0.14"/>
    <path d="M586 366 C640 312 704 300 760 320 L760 420 L548 420 C546 400 558 384 586 366Z" fill="#fde68a" opacity="0.24"/>
    <rect x="34" y="34" width="692" height="352" rx="24" fill="url(#panel)" filter="url(#shadow)"/>
    <rect x="54" y="54" width="96" height="96" rx="48" fill="#e0f2fe"/>
    <image x="54" y="54" width="96" height="96" href="{}" preserveAspectRatio="xMidYMid slice" clip-path="url(#avatarClip)"/>
    <circle cx="102" cy="102" r="50" fill="none" stroke="#ffffff" stroke-width="5"/>
    <g transform="translate(166 56)">
        <text x="0" y="22" fill="#64748b" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="20" font-weight="700">{}</text>
        <text x="0" y="64" fill="#0f172a" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="35" font-weight="850">{}</text>
        <path d="M3 88h13a10 10 0 0 0 10-10v-4M13 78l-10 10 10 10" fill="none" stroke="#2563eb" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
        <text x="36" y="96" fill="#2563eb" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="18">{}</text>
    </g>
    <text x="58" y="206" fill="#334155" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="18" font-weight="800">ABOUT</text>
    <text clip-path="url(#aboutClip)" fill="#475569" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="18" font-weight="500">{}</text>
    <rect x="54" y="294" width="150" height="62" rx="14" fill="#ecfeff"/>
    <rect x="220" y="294" width="150" height="62" rx="14" fill="#f0fdf4"/>
    <rect x="386" y="294" width="150" height="62" rx="14" fill="#fff7ed"/>
    <rect x="552" y="294" width="136" height="62" rx="14" fill="#fdf2f8"/>
    <path d="M82 312l4.5 9.2 10.1 1.5-7.3 7.1 1.7 10-9-4.7-9 4.7 1.7-10-7.3-7.1 10.1-1.5z" fill="#0891b2"/>
    <text x="104" y="318" fill="#0e7490" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="14" font-weight="800">STARS</text>
    <text x="104" y="342" fill="#0f172a" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="24" font-weight="850">{}</text>
    <path d="M250 313h14v14h-14zM264 320h10v14h-14v-7" fill="none" stroke="#16a34a" stroke-width="3" stroke-linejoin="round"/>
    <text x="274" y="318" fill="#15803d" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="14" font-weight="800">FORKS</text>
    <text x="274" y="342" fill="#0f172a" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="24" font-weight="850">{}</text>
    <circle cx="418" cy="324" r="13" fill="none" stroke="#ea580c" stroke-width="3"/><path d="M418 316v11M418 333h.1" stroke="#ea580c" stroke-width="3" stroke-linecap="round"/>
    <text x="440" y="318" fill="#c2410c" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="14" font-weight="800">ISSUES</text>
    <text x="440" y="342" fill="#0f172a" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="24" font-weight="850">{}</text>
    <path d="M579 313v24M567 325h24M573 319l6-6 6 6M573 331l6 6 6-6" fill="none" stroke="#be185d" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
    <text x="602" y="318" fill="#be185d" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="14" font-weight="800">RECENT</text>
    <text x="602" y="342" fill="#0f172a" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="18" font-weight="850">{}</text>
</svg>"##,
        escape_html(avatar_href),
        escape_html(&card.owner),
        escape_html(&card.name),
        escape_html(&truncate_middle(&card.url, 58)),
        about_lines,
        format_count(card.stars),
        format_count(card.forks),
        format_count(card.issues),
        escape_html(&recent),
    )
}

pub fn render_repo_card_png(card: &RepositoryCard) -> anyhow::Result<Vec<u8>> {
    svg_to_png(&render_repo_card_svg(card))
}

pub fn render_change_card_svg(card: &ChangeCard) -> String {
    let commit_rows = render_change_commit_rows(&card.commits);
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="820" height="460" viewBox="0 0 820 460">
  <defs>
    <linearGradient id="changeBg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#0891b2"/>
      <stop offset="0.52" stop-color="#2563eb"/>
      <stop offset="1" stop-color="#7c3aed"/>
    </linearGradient>
    <linearGradient id="changePanel" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#ffffff" stop-opacity="0.98"/>
      <stop offset="1" stop-color="#f8fafc" stop-opacity="0.94"/>
    </linearGradient>
    <filter id="changeShadow" x="-10%" y="-10%" width="120%" height="130%">
      <feDropShadow dx="0" dy="18" stdDeviation="18" flood-color="#0f172a" flood-opacity="0.26"/>
    </filter>
  </defs>
    <rect width="820" height="460" rx="28" fill="url(#changeBg)"/>
    <path d="M70 96 C142 26 260 24 342 82 C436 150 520 110 606 56 C668 17 744 35 792 94 L792 0 L0 0 L0 154 C20 137 42 117 70 96Z" fill="#ffffff" opacity="0.14"/>
    <path d="M650 388 C696 346 752 338 820 366 L820 460 L600 460 C600 430 616 408 650 388Z" fill="#fde68a" opacity="0.23"/>
    <rect x="38" y="38" width="744" height="384" rx="22" fill="url(#changePanel)" filter="url(#changeShadow)"/>
    <rect x="64" y="66" width="146" height="38" rx="19" fill="#dbeafe"/>
    <path d="M85 77v11h11M96 88l-14 14M100 76h16v16" fill="none" stroke="#1d4ed8" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
    <text x="124" y="91" fill="#1d4ed8" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="17" font-weight="800">CHANGE</text>
    <text x="64" y="146" fill="#0f172a" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="34" font-weight="850">{}</text>
    <text x="64" y="181" fill="#475569" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="20" font-weight="650">{}</text>
    <rect x="64" y="216" width="168" height="68" rx="14" fill="#eff6ff"/>
    <rect x="248" y="216" width="168" height="68" rx="14" fill="#f0fdf4"/>
    <rect x="432" y="216" width="286" height="68" rx="14" fill="#fdf2f8"/>
    <path d="M88 238l8 8-8 8M106 254h16" fill="none" stroke="#1d4ed8" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
    <text x="86" y="269" fill="#0f172a" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="22" font-weight="850">{}</text>
    <circle cx="278" cy="246" r="11" fill="none" stroke="#15803d" stroke-width="3"/><path d="M278 257v8M266 265h24" stroke="#15803d" stroke-width="3" stroke-linecap="round"/>
    <text x="302" y="269" fill="#0f172a" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="22" font-weight="850">{}</text>
    <path d="M456 236h22v20h-22zM462 242h10" fill="none" stroke="#be185d" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
    <text x="486" y="269" fill="#0f172a" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="20" font-weight="850">{}</text>
    <text x="64" y="326" fill="#334155" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="20" font-weight="750">RECENT COMMITS</text>
  {}
  <text x="64" y="398" fill="#2563eb" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="17">{}</text>
</svg>"##,
        escape_html(&truncate_for_card(&card.title, 42)),
        escape_html(&truncate_for_card(&card.repository, 56)),
        escape_html(&truncate_for_card(&card.branch, 18)),
        escape_html(&truncate_for_card(&card.actor, 18)),
        escape_html(&truncate_for_card(&card.summary, 30)),
        commit_rows,
        escape_html(&truncate_for_card(&card.url, 78)),
    )
}

pub fn render_change_card_png(card: &ChangeCard) -> anyhow::Result<Vec<u8>> {
    svg_to_png(&render_change_card_svg(card))
}

pub fn change_card_query(card: &ChangeCard) -> String {
    let commits = card
        .commits
        .iter()
        .take(3)
        .map(|commit| format!("{} — {}", commit.message, commit.author))
        .collect::<Vec<_>>()
        .join("\n");
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("title", &card.title)
        .append_pair("repository", &card.repository)
        .append_pair("branch", &card.branch)
        .append_pair("actor", &card.actor)
        .append_pair("summary", &card.summary)
        .append_pair("url", &card.url)
        .append_pair("commits", &commits)
        .finish()
}

pub fn change_card_from_query(query: &str) -> ChangeCard {
    let pairs = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect::<std::collections::HashMap<_, _>>();
    let commits = pairs
        .get("commits")
        .map(|value| {
            value
                .lines()
                .take(3)
                .map(|line| ChangeCommit {
                    message: line.to_string(),
                    author: String::new(),
                    url: String::new(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    ChangeCard {
        title: pairs.get("title").cloned().unwrap_or_default(),
        repository: pairs.get("repository").cloned().unwrap_or_default(),
        branch: pairs.get("branch").cloned().unwrap_or_default(),
        actor: pairs.get("actor").cloned().unwrap_or_default(),
        summary: pairs.get("summary").cloned().unwrap_or_default(),
        url: pairs.get("url").cloned().unwrap_or_default(),
        commits,
    }
}

fn render_change_commit_rows(commits: &[ChangeCommit]) -> String {
    if commits.is_empty() {
        return r##"<text x="64" y="356" fill="#64748b" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="18">No commit detail available.</text>"##.to_string();
    }

    commits
        .iter()
        .take(3)
        .enumerate()
        .map(|(index, commit)| {
            let y = 356 + index * 25;
            let author = if commit.author.is_empty() {
                String::new()
            } else {
                format!(" · {}", commit.author)
            };
            format!(
                r##"<text x="64" y="{}" fill="#0f172a" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="18" font-weight="650">{}</text>"##,
                y,
                escape_html(&truncate_for_card(
                    &format!("{}{}", commit.message, author),
                    76,
                ))
            )
        })
        .collect::<Vec<_>>()
        .join("\n  ")
}

fn svg_to_png(svg: &str) -> anyhow::Result<Vec<u8>> {
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(svg, &options)?;
    let size = tree.size().to_int_size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| anyhow::anyhow!("invalid svg size"))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|error| anyhow::anyhow!("encode png failed: {error}"))
}

fn svg_text_lines(
    value: &str,
    max_chars: usize,
    max_lines: usize,
    x: u32,
    y: u32,
    line_height: u32,
    indent: u32,
) -> String {
    let lines = wrap_text(value, max_chars, max_lines);
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            format!(
                r##"<tspan x="{}" y="{}">{}</tspan>"##,
                x + if index == 0 { 0 } else { indent },
                y + index as u32 * line_height,
                escape_html(line)
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn wrap_text(value: &str, max_chars: usize, max_lines: usize) -> Vec<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return vec!["No description provided.".to_string()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut truncated = false;

    'words: for word in split_wrappable(trimmed) {
        let mut remaining = word.as_str();
        let mut needs_separator = !current.is_empty() && word.chars().all(|ch| ch.is_ascii());

        while !remaining.is_empty() {
            let separator_len = usize::from(needs_separator);
            let current_len = current.chars().count();
            if current_len + separator_len >= max_chars {
                lines.push(current);
                current = String::new();
                needs_separator = false;
                if lines.len() == max_lines {
                    truncated = true;
                    break 'words;
                }
                continue;
            }

            let available = max_chars - current_len - separator_len;
            let remaining_len = remaining.chars().count();
            let take_len = available.min(remaining_len);
            let chunk = remaining.chars().take(take_len).collect::<String>();
            if needs_separator {
                current.push(' ');
            }
            current.push_str(&chunk);
            remaining = &remaining[chunk.len()..];
            needs_separator = false;

            if !remaining.is_empty() {
                lines.push(current);
                current = String::new();
                if lines.len() == max_lines {
                    truncated = true;
                    break 'words;
                }
            }
        }
    }
    if !current.is_empty() && lines.len() < max_lines {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(truncate_for_card(trimmed, max_chars));
    }
    if truncated && let Some(last) = lines.last_mut() {
        *last = truncate_for_card(last, max_chars.saturating_sub(1));
    }
    lines
}

fn split_wrappable(value: &str) -> Vec<String> {
    if value.chars().any(|ch| !ch.is_ascii()) {
        value.chars().map(|ch| ch.to_string()).collect()
    } else {
        value.split_whitespace().map(ToString::to_string).collect()
    }
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

fn truncate_middle(value: &str, max_chars: usize) -> String {
    let total = value.chars().count();
    if total <= max_chars || max_chars < 5 {
        return value.to_string();
    }

    let head_len = (max_chars - 1) / 2;
    let tail_len = max_chars - 1 - head_len;
    let head = value.chars().take(head_len).collect::<String>();
    let tail = value
        .chars()
        .rev()
        .take(tail_len)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{head}…{tail}")
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
        card: None,
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
        card: None,
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
    let commits = value["commits"]
        .as_array()
        .map(|commits| {
            commits
                .iter()
                .take(3)
                .map(|commit| ChangeCommit {
                    message: commit["message"]
                        .as_str()
                        .unwrap_or("unknown commit")
                        .lines()
                        .next()
                        .unwrap_or("unknown commit")
                        .to_string(),
                    author: commit["author"]["name"]
                        .as_str()
                        .or_else(|| commit["author"]["username"].as_str())
                        .unwrap_or(actor)
                        .to_string(),
                    url: commit["url"].as_str().unwrap_or(compare).to_string(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let message = format!(
        "新的代码提交\n仓库: {}\n分支: {}\n提交数: {}\n提交者: {}\n{}",
        repository.full_name, branch, count, actor, compare
    );
    Notification {
        repository: repository.full_name.clone(),
        feature: Feature::Pushes,
        message,
        card: Some(ChangeCard {
            title: "新的代码提交".to_string(),
            repository: repository.full_name.clone(),
            branch: branch.to_string(),
            actor: actor.to_string(),
            summary: format!("{} commits pushed", count),
            url: compare.to_string(),
            commits,
        }),
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
        card: None,
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
        card: None,
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
        card: None,
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
        card: None,
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
        card: None,
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
            avatar_data_uri: None,
            about: "一个好用的项目".to_string(),
            stars: 15320,
            forks: 821,
            issues: 12,
            recent_commit_time: "2026-06-29".to_string(),
        };
        let svg = render_repo_card_svg(&card);

        assert!(svg.contains("owner"));
        assert!(svg.contains("project"));
        assert!(svg.contains("15.3K"));
        assert!(svg.contains("ISSUES"));
        assert!(svg.contains("一个"));
        assert!(svg.contains("<path"));
    }

    #[test]
    fn wraps_long_repo_card_about_text() {
        let card = RepositoryCard {
            owner: "owner".to_string(),
            name: "project".to_string(),
            full_name: "owner/project".to_string(),
            url: "https://github.com/owner/project".to_string(),
            avatar_url: "https://avatars.githubusercontent.com/u/1?v=4".to_string(),
            avatar_data_uri: None,
            about: "supercalifragilisticexpialidocioussupercalifragilisticexpialidocious repository automation".to_string(),
            stars: 15320,
            forks: 821,
            issues: 12,
            recent_commit_time: "2026-06-29".to_string(),
        };

        let svg = render_repo_card_svg(&card);

        assert!(svg.contains("clipPath id=\"aboutClip\""));
        assert!(svg.contains("clip-path=\"url(#aboutClip)\""));
        assert!(svg.contains(
            r#"<tspan x="58" y="238">supercalifragilisticexpialidocioussupercalifragi</tspan>"#
        ));
        assert!(svg.contains(
            r#"<tspan x="58" y="259">listicexpialidocious repository automation</tspan>"#
        ));
    }

    #[test]
    fn renders_repo_card_png() {
        let card = RepositoryCard {
            owner: "owner".to_string(),
            name: "project".to_string(),
            full_name: "owner/project".to_string(),
            url: "https://github.com/owner/project".to_string(),
            avatar_url: "https://avatars.githubusercontent.com/u/1?v=4".to_string(),
            avatar_data_uri: None,
            about: "一个好用的项目".to_string(),
            stars: 15320,
            forks: 821,
            issues: 12,
            recent_commit_time: "2026-06-29".to_string(),
        };

        let png = render_repo_card_png(&card).unwrap();

        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
    }

    #[test]
    fn renders_change_card_png() {
        let card = ChangeCard {
            title: "新的代码提交".to_string(),
            repository: "owner/project".to_string(),
            branch: "main".to_string(),
            actor: "alice".to_string(),
            summary: "2 commits pushed".to_string(),
            url: "https://github.com/owner/project/compare/a...b".to_string(),
            commits: vec![ChangeCommit {
                message: "fix card".to_string(),
                author: "alice".to_string(),
                url: "https://github.com/owner/project/commit/b".to_string(),
            }],
        };

        let png = render_change_card_png(&card).unwrap();

        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
    }
}
