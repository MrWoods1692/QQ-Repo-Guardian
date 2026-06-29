use serde::Deserialize;
use serde_json::Value;
use std::io::Cursor;

use crate::config::FeatureConfig;

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

pub fn render_repo_card_png(card: &RepositoryCard) -> anyhow::Result<Vec<u8>> {
    let mut canvas = PngCanvas::new(760, 420, [238, 242, 255, 255]);
    canvas.gradient([14, 165, 233, 255], [236, 72, 153, 255]);
    canvas.rect(36, 36, 688, 348, [250, 252, 255, 244]);
    canvas.rect(54, 54, 92, 92, [219, 234, 254, 255]);
    canvas.text(74, 92, "GH", 4, [29, 78, 216, 255]);
    canvas.text(164, 74, &card.owner, 3, [100, 116, 139, 255]);
    canvas.text(164, 112, &card.name, 5, [15, 23, 42, 255]);
    canvas.text(164, 150, &card.url, 2, [37, 99, 235, 255]);
    canvas.text(54, 214, "ABOUT", 3, [51, 65, 85, 255]);
    canvas.text(
        54,
        246,
        &truncate_for_card(&card.about, 82),
        3,
        [71, 85, 105, 255],
    );
    draw_metric(
        &mut canvas,
        54,
        286,
        "STARS",
        &format_count(card.stars),
        [239, 246, 255, 255],
        [29, 78, 216, 255],
    );
    draw_metric(
        &mut canvas,
        218,
        286,
        "FORKS",
        &format_count(card.forks),
        [240, 253, 244, 255],
        [21, 128, 61, 255],
    );
    draw_metric(
        &mut canvas,
        382,
        286,
        "ISSUES",
        &format_count(card.issues),
        [255, 247, 237, 255],
        [194, 65, 12, 255],
    );
    draw_metric(
        &mut canvas,
        546,
        286,
        "RECENT",
        &card.recent_commit_time,
        [253, 242, 248, 255],
        [190, 24, 93, 255],
    );
    canvas.finish_png()
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
  <circle cx="706" cy="356" r="118" fill="#fef3c7" opacity="0.18"/>
  <rect x="38" y="38" width="744" height="384" rx="22" fill="url(#changePanel)" filter="url(#changeShadow)"/>
  <rect x="64" y="66" width="132" height="38" rx="19" fill="#dbeafe"/>
  <text x="84" y="91" fill="#1d4ed8" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="18" font-weight="800">Repo Change</text>
  <text x="64" y="146" fill="#0f172a" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="35" font-weight="850">{}</text>
  <text x="64" y="181" fill="#475569" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="20" font-weight="650">{}</text>
  <rect x="64" y="216" width="168" height="68" rx="16" fill="#eff6ff"/>
  <rect x="248" y="216" width="168" height="68" rx="16" fill="#f0fdf4"/>
  <rect x="432" y="216" width="286" height="68" rx="16" fill="#fdf2f8"/>
  <text x="86" y="245" fill="#1d4ed8" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="16" font-weight="750">Branch</text>
  <text x="86" y="269" fill="#0f172a" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="22" font-weight="850">{}</text>
  <text x="270" y="245" fill="#15803d" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="16" font-weight="750">Actor</text>
  <text x="270" y="269" fill="#0f172a" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="22" font-weight="850">{}</text>
  <text x="454" y="245" fill="#be185d" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="16" font-weight="750">Summary</text>
  <text x="454" y="269" fill="#0f172a" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="21" font-weight="850">{}</text>
  <text x="64" y="326" fill="#334155" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="20" font-weight="750">Recent commits</text>
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
    let mut canvas = PngCanvas::new(820, 460, [238, 242, 255, 255]);
    canvas.gradient([8, 145, 178, 255], [124, 58, 237, 255]);
    canvas.rect(38, 38, 744, 384, [250, 252, 255, 246]);
    canvas.rect(64, 66, 156, 38, [219, 234, 254, 255]);
    canvas.text(84, 91, "REPO CHANGE", 2, [29, 78, 216, 255]);
    canvas.text(64, 146, &card.title, 5, [15, 23, 42, 255]);
    canvas.text(64, 181, &card.repository, 3, [71, 85, 105, 255]);
    draw_metric(
        &mut canvas,
        64,
        216,
        "BRANCH",
        &card.branch,
        [239, 246, 255, 255],
        [29, 78, 216, 255],
    );
    draw_metric(
        &mut canvas,
        248,
        216,
        "ACTOR",
        &card.actor,
        [240, 253, 244, 255],
        [21, 128, 61, 255],
    );
    canvas.rect(432, 216, 286, 68, [253, 242, 248, 255]);
    canvas.text(454, 245, "SUMMARY", 2, [190, 24, 93, 255]);
    canvas.text(454, 269, &card.summary, 3, [15, 23, 42, 255]);
    canvas.text(64, 326, "RECENT COMMITS", 3, [51, 65, 85, 255]);
    if card.commits.is_empty() {
        canvas.text(
            64,
            356,
            "No commit detail available.",
            3,
            [100, 116, 139, 255],
        );
    } else {
        for (index, commit) in card.commits.iter().take(3).enumerate() {
            canvas.text(
                64,
                356 + index as u32 * 25,
                &format!("{} - {}", commit.message, commit.author),
                3,
                [15, 23, 42, 255],
            );
        }
    }
    canvas.text(64, 398, &card.url, 2, [37, 99, 235, 255]);
    canvas.finish_png()
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

fn draw_metric(
    canvas: &mut PngCanvas,
    x: u32,
    y: u32,
    label: &str,
    value: &str,
    background: [u8; 4],
    accent: [u8; 4],
) {
    canvas.rect(x, y, 146, 64, background);
    canvas.text(x + 22, y + 28, label, 2, accent);
    canvas.text(x + 22, y + 54, value, 3, [15, 23, 42, 255]);
}

struct PngCanvas {
    image: image::RgbaImage,
}

impl PngCanvas {
    fn new(width: u32, height: u32, color: [u8; 4]) -> Self {
        Self {
            image: image::RgbaImage::from_pixel(width, height, image::Rgba(color)),
        }
    }

    fn gradient(&mut self, start: [u8; 4], end: [u8; 4]) {
        let width = self.image.width().saturating_sub(1).max(1);
        let height = self.image.height().saturating_sub(1).max(1);
        for y in 0..self.image.height() {
            for x in 0..self.image.width() {
                let ratio = (x + y) as f32 / (width + height) as f32;
                let color = blend(start, end, ratio);
                self.image.put_pixel(x, y, image::Rgba(color));
            }
        }
    }

    fn rect(&mut self, x: u32, y: u32, width: u32, height: u32, color: [u8; 4]) {
        let max_x = (x + width).min(self.image.width());
        let max_y = (y + height).min(self.image.height());
        for py in y..max_y {
            for px in x..max_x {
                self.image.put_pixel(px, py, image::Rgba(color));
            }
        }
    }

    fn text(&mut self, x: u32, y: u32, value: &str, scale: u32, color: [u8; 4]) {
        let mut cursor = x;
        for ch in normalize_text(value).chars().take(96) {
            if ch == ' ' {
                cursor += 4 * scale;
                continue;
            }
            draw_char(
                &mut self.image,
                cursor,
                y.saturating_sub(7 * scale),
                ch,
                scale,
                color,
            );
            cursor += 6 * scale;
            if cursor + 5 * scale >= self.image.width() {
                break;
            }
        }
    }

    fn finish_png(self) -> anyhow::Result<Vec<u8>> {
        let mut output = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(self.image)
            .write_to(&mut output, image::ImageOutputFormat::Png)?;
        Ok(output.into_inner())
    }
}

fn blend(start: [u8; 4], end: [u8; 4], ratio: f32) -> [u8; 4] {
    let mut color = [0; 4];
    for index in 0..4 {
        color[index] =
            (start[index] as f32 + (end[index] as f32 - start[index] as f32) * ratio) as u8;
    }
    color
}

fn normalize_text(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii() { ch } else { '?' })
        .collect()
}

fn draw_char(image: &mut image::RgbaImage, x: u32, y: u32, ch: char, scale: u32, color: [u8; 4]) {
    for (row_index, row) in glyph(ch).iter().enumerate() {
        for col in 0..5 {
            if row & (1 << (4 - col)) == 0 {
                continue;
            }
            let px = x + col * scale;
            let py = y + row_index as u32 * scale;
            for sy in 0..scale {
                for sx in 0..scale {
                    let target_x = px + sx;
                    let target_y = py + sy;
                    if target_x < image.width() && target_y < image.height() {
                        image.put_pixel(target_x, target_y, image::Rgba(color));
                    }
                }
            }
        }
    }
}

fn glyph(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'B' => [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e],
        'C' => [0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e],
        'D' => [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e],
        'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        'G' => [0x0e, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0f],
        'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'I' => [0x0e, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e],
        'J' => [0x01, 0x01, 0x01, 0x01, 0x11, 0x11, 0x0e],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
        'M' => [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        'Q' => [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d],
        'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a],
        'X' => [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f],
        '0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        '1' => [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
        '2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        '3' => [0x1e, 0x01, 0x01, 0x0e, 0x01, 0x01, 0x1e],
        '4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        '5' => [0x1f, 0x10, 0x10, 0x1e, 0x01, 0x01, 0x1e],
        '6' => [0x0e, 0x10, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        '7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        '9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x01, 0x0e],
        '/' => [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
        ':' => [0x00, 0x04, 0x04, 0x00, 0x04, 0x04, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x0c],
        '-' => [0x00, 0x00, 0x00, 0x1f, 0x00, 0x00, 0x00],
        '_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1f],
        '#' => [0x0a, 0x1f, 0x0a, 0x0a, 0x1f, 0x0a, 0x00],
        '?' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04],
        _ => [0x00, 0x00, 0x0e, 0x01, 0x0f, 0x11, 0x0f],
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

    #[test]
    fn renders_repo_card_png() {
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
