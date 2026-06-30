use std::sync::Arc;

use anyhow::Context;
use scraper::{Html, Selector};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::{TlsConnector, rustls};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkPreview {
    pub url: String,
    pub host: String,
    pub title: String,
    pub description: String,
    pub site_name: String,
    pub content_type: String,
    pub image_url: Option<String>,
    pub video_url: Option<String>,
    pub ssl_issuer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkPreviewCard {
    pub url: String,
    pub host: String,
    pub title: String,
    pub description: String,
    pub site_name: String,
    pub content_type: String,
    pub image_url: Option<String>,
    pub video_url: Option<String>,
    pub ssl_issuer: Option<String>,
}

pub fn extract_first_url(message: &str) -> Option<String> {
    message
        .split_whitespace()
        .filter_map(clean_url_candidate)
        .find(|url| matches!(url.scheme(), "http" | "https"))
        .map(Url::into)
}

pub async fn fetch_link_preview(
    client: &reqwest::Client,
    url: &str,
) -> anyhow::Result<LinkPreview> {
    let parsed = Url::parse(url).context("invalid link url")?;
    anyhow::ensure!(
        matches!(parsed.scheme(), "http" | "https"),
        "unsupported link scheme"
    );
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("missing link host"))?
        .to_string();

    let response = client
        .get(parsed.clone())
        .header(reqwest::header::USER_AGENT, "qq-repo-guardian link-preview")
        .send()
        .await?
        .error_for_status()?;
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unknown")
        .split(';')
        .next()
        .unwrap_or("unknown")
        .trim()
        .to_string();

    let text = if content_type.contains("html") || content_type == "unknown" {
        response.text().await.unwrap_or_default()
    } else {
        String::new()
    };
    let metadata = parse_link_metadata(&parsed, &text, &content_type);
    let ssl_issuer = if parsed.scheme() == "https" {
        fetch_ssl_issuer(&host, parsed.port_or_known_default().unwrap_or(443)).await
    } else {
        None
    };

    Ok(LinkPreview {
        url: parsed.to_string(),
        host,
        title: metadata.title,
        description: metadata.description,
        site_name: metadata.site_name,
        content_type,
        image_url: metadata.image_url,
        video_url: metadata.video_url,
        ssl_issuer,
    })
}

pub fn link_preview_card_query(card: &LinkPreviewCard) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer
        .append_pair("url", &card.url)
        .append_pair("host", &card.host)
        .append_pair("title", &card.title)
        .append_pair("description", &card.description)
        .append_pair("site_name", &card.site_name)
        .append_pair("content_type", &card.content_type);
    if let Some(image_url) = &card.image_url {
        serializer.append_pair("image", image_url);
    }
    if let Some(video_url) = &card.video_url {
        serializer.append_pair("video", video_url);
    }
    if let Some(ssl_issuer) = &card.ssl_issuer {
        serializer.append_pair("ssl", ssl_issuer);
    }
    serializer.finish()
}

pub fn link_preview_card_from_query(query: &str) -> LinkPreviewCard {
    let pairs = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect::<std::collections::HashMap<_, _>>();
    LinkPreviewCard {
        url: pairs.get("url").cloned().unwrap_or_default(),
        host: pairs.get("host").cloned().unwrap_or_default(),
        title: pairs.get("title").cloned().unwrap_or_default(),
        description: pairs.get("description").cloned().unwrap_or_default(),
        site_name: pairs.get("site_name").cloned().unwrap_or_default(),
        content_type: pairs.get("content_type").cloned().unwrap_or_default(),
        image_url: pairs.get("image").cloned(),
        video_url: pairs.get("video").cloned(),
        ssl_issuer: pairs.get("ssl").cloned(),
    }
}

pub fn render_link_preview_png(card: &LinkPreviewCard) -> anyhow::Result<Vec<u8>> {
    crate::github::svg_to_png(&render_link_preview_svg(card))
}

pub fn render_link_preview_svg(card: &LinkPreviewCard) -> String {
    let title = truncate_for_card(non_empty(&card.title, "网页链接预览"), 30);
    let description = non_empty(
        &card.description,
        "未获取到页面描述，可打开原链接查看详情。",
    );
    let description_lines = svg_text_lines(description, 42, 3, 66, 230, 26);
    let site_name = truncate_for_card(non_empty(&card.site_name, &card.host), 22);
    let content_type = truncate_for_card(non_empty(&card.content_type, "unknown"), 24);
    let ssl_issuer =
        truncate_for_card(card.ssl_issuer.as_deref().unwrap_or("未获取到证书机构"), 30);
    let media_text = match (&card.image_url, &card.video_url) {
        (Some(_), Some(_)) => "已提取图片与视频",
        (Some(_), None) => "已提取图片",
        (None, Some(_)) => "已提取视频",
        (None, None) => "未发现媒体",
    };
    let image_note = card
        .image_url
        .as_deref()
        .map(|value| truncate_middle(value, 42))
        .unwrap_or_else(|| "无图片链接".to_string());
    let video_note = card
        .video_url
        .as_deref()
        .map(|value| truncate_middle(value, 42))
        .unwrap_or_else(|| "无视频链接".to_string());

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="820" height="470" viewBox="0 0 820 470">
  <defs>
    <linearGradient id="linkBg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#047857"/>
      <stop offset="0.48" stop-color="#2563eb"/>
      <stop offset="1" stop-color="#c2410c"/>
    </linearGradient>
    <filter id="linkShadow" x="-10%" y="-10%" width="120%" height="130%">
      <feDropShadow dx="0" dy="18" stdDeviation="18" flood-color="#0f172a" flood-opacity="0.25"/>
    </filter>
  </defs>
  <rect width="820" height="470" rx="30" fill="url(#linkBg)"/>
  <path d="M0 118 C96 48 176 44 284 92 C392 140 470 112 574 54 C672 0 756 18 820 70 L820 0 L0 0Z" fill="#ffffff" opacity="0.15"/>
  <rect x="40" y="40" width="740" height="390" rx="24" fill="#ffffff" opacity="0.97" filter="url(#linkShadow)"/>
  <rect x="40" y="40" width="740" height="7" rx="3" fill="#22c55e"/>
  <rect x="66" y="70" width="150" height="38" rx="19" fill="#ecfeff"/>
  <circle cx="91" cy="89" r="10" fill="#0891b2"/>
  <text x="112" y="96" fill="#0e7490" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="16" font-weight="850">链接解析</text>
  <text x="66" y="151" fill="#0f172a" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="34" font-weight="850">{}</text>
  <text x="66" y="184" fill="#475569" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="18" font-weight="650">{}</text>
  <text fill="#334155" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="19" font-weight="650">{}</text>
  <rect x="66" y="314" width="212" height="76" rx="15" fill="#eff6ff"/>
  <rect x="304" y="314" width="212" height="76" rx="15" fill="#f0fdf4"/>
  <rect x="542" y="314" width="174" height="76" rx="15" fill="#fff7ed"/>
  <text x="92" y="342" fill="#1d4ed8" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="14" font-weight="850">SEO / 站点</text>
  <text x="92" y="368" fill="#0f172a" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="21" font-weight="850">{}</text>
  <text x="330" y="342" fill="#15803d" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="14" font-weight="850">媒体提取</text>
  <text x="330" y="368" fill="#0f172a" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="21" font-weight="850">{}</text>
  <text x="568" y="342" fill="#c2410c" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="14" font-weight="850">内容类型</text>
  <text x="568" y="368" fill="#0f172a" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="18" font-weight="850">{}</text>
  <rect x="66" y="402" width="650" height="1" fill="#e2e8f0"/>
  <text x="66" y="424" fill="#64748b" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="14" font-weight="700">SSL 认证机构：{}</text>
  <text x="430" y="424" fill="#64748b" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="13" font-weight="650">{}</text>
  <title>{}</title>
  <desc>图片：{} 视频：{}</desc>
</svg>"##,
        escape_html(&title),
        escape_html(&truncate_middle(&card.url, 72)),
        description_lines,
        escape_html(&site_name),
        escape_html(media_text),
        escape_html(&content_type),
        escape_html(&ssl_issuer),
        escape_html(&card.host),
        escape_html(&title),
        escape_html(&image_note),
        escape_html(&video_note),
    )
}

impl From<LinkPreview> for LinkPreviewCard {
    fn from(preview: LinkPreview) -> Self {
        Self {
            url: preview.url,
            host: preview.host,
            title: preview.title,
            description: preview.description,
            site_name: preview.site_name,
            content_type: preview.content_type,
            image_url: preview.image_url,
            video_url: preview.video_url,
            ssl_issuer: preview.ssl_issuer,
        }
    }
}

#[derive(Debug, Default)]
struct LinkMetadata {
    title: String,
    description: String,
    site_name: String,
    image_url: Option<String>,
    video_url: Option<String>,
}

fn parse_link_metadata(base_url: &Url, html: &str, content_type: &str) -> LinkMetadata {
    if html.trim().is_empty() {
        return LinkMetadata {
            title: base_url.host_str().unwrap_or("网页链接").to_string(),
            description: format!("非 HTML 内容：{content_type}"),
            site_name: base_url.host_str().unwrap_or_default().to_string(),
            image_url: None,
            video_url: None,
        };
    }

    let document = Html::parse_document(html);
    let title = meta_content(&document, "property", "og:title")
        .or_else(|| meta_content(&document, "name", "twitter:title"))
        .or_else(|| title_text(&document))
        .unwrap_or_else(|| base_url.host_str().unwrap_or("网页链接").to_string());
    let description = meta_content(&document, "property", "og:description")
        .or_else(|| meta_content(&document, "name", "description"))
        .or_else(|| meta_content(&document, "name", "twitter:description"))
        .unwrap_or_else(|| "未获取到页面描述".to_string());
    let site_name = meta_content(&document, "property", "og:site_name")
        .unwrap_or_else(|| base_url.host_str().unwrap_or_default().to_string());
    let image_url = meta_content(&document, "property", "og:image")
        .or_else(|| meta_content(&document, "name", "twitter:image"))
        .or_else(|| first_attr(&document, "img", "src"))
        .and_then(|value| absolutize_url(base_url, &value));
    let video_url = meta_content(&document, "property", "og:video")
        .or_else(|| meta_content(&document, "property", "og:video:url"))
        .or_else(|| meta_content(&document, "property", "og:video:secure_url"))
        .or_else(|| first_attr(&document, "video source", "src"))
        .or_else(|| first_attr(&document, "video", "src"))
        .and_then(|value| absolutize_url(base_url, &value));

    LinkMetadata {
        title: normalize_whitespace(&title),
        description: normalize_whitespace(&description),
        site_name: normalize_whitespace(&site_name),
        image_url,
        video_url,
    }
}

fn meta_content(document: &Html, attr: &str, expected: &str) -> Option<String> {
    let selector = Selector::parse("meta").ok()?;
    document.select(&selector).find_map(|element| {
        let value = element.value().attr(attr)?;
        if value.eq_ignore_ascii_case(expected) {
            element.value().attr("content").map(str::to_string)
        } else {
            None
        }
    })
}

fn title_text(document: &Html) -> Option<String> {
    let selector = Selector::parse("title").ok()?;
    document
        .select(&selector)
        .next()
        .map(|element| normalize_whitespace(&element.text().collect::<Vec<_>>().join(" ")))
        .filter(|value| !value.is_empty())
}

fn first_attr(document: &Html, selector: &str, attr: &str) -> Option<String> {
    let selector = Selector::parse(selector).ok()?;
    document
        .select(&selector)
        .find_map(|element| element.value().attr(attr).map(str::to_string))
}

fn clean_url_candidate(value: &str) -> Option<Url> {
    let trimmed = value.trim_matches(|ch: char| {
        ch.is_whitespace()
            || matches!(
                ch,
                '<' | '>' | '"' | '\'' | '（' | '）' | '(' | ')' | '[' | ']'
            )
    });
    let trimmed = trimmed.trim_end_matches(|ch: char| {
        matches!(ch, '。' | '，' | ',' | '.' | '!' | '?' | '！' | '？')
    });
    Url::parse(trimmed).ok()
}

fn absolutize_url(base_url: &Url, value: &str) -> Option<String> {
    base_url.join(value.trim()).ok().map(Url::into)
}

async fn fetch_ssl_issuer(host: &str, port: u16) -> Option<String> {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let server_name = rustls_pki_types::ServerName::try_from(host.to_string()).ok()?;
    let tcp = TcpStream::connect((host, port)).await.ok()?;
    let mut stream = connector.connect(server_name, tcp).await.ok()?;
    let _ = stream.write_all(b"HEAD / HTTP/1.0\r\n\r\n").await;
    let mut buffer = [0_u8; 1];
    let _ = stream.read(&mut buffer).await;
    let certificates = stream.get_ref().1.peer_certificates()?;
    let certificate = certificates.first()?;
    certificate_issuer(certificate.as_ref())
}

fn certificate_issuer(der: &[u8]) -> Option<String> {
    let (_, certificate) = x509_parser::parse_x509_certificate(der).ok()?;
    for attr in certificate.issuer().iter_organization() {
        if let Ok(value) = attr.as_str()
            && !value.trim().is_empty()
        {
            return Some(value.trim().to_string());
        }
    }
    for attr in certificate.issuer().iter_common_name() {
        if let Ok(value) = attr.as_str()
            && !value.trim().is_empty()
        {
            return Some(value.trim().to_string());
        }
    }
    Some(certificate.issuer().to_string())
}

fn svg_text_lines(
    value: &str,
    max_chars: usize,
    max_lines: usize,
    x: u32,
    y: u32,
    line_height: u32,
) -> String {
    wrap_text(value, max_chars, max_lines)
        .iter()
        .enumerate()
        .map(|(index, line)| {
            format!(
                r##"<tspan x="{}" y="{}">{}</tspan>"##,
                x,
                y + index as u32 * line_height,
                escape_html(line)
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn wrap_text(value: &str, max_chars: usize, max_lines: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in split_wrappable(value) {
        if current.chars().count() + word.chars().count() > max_chars && !current.is_empty() {
            lines.push(current);
            current = String::new();
            if lines.len() == max_lines {
                break;
            }
        }
        current.push_str(&word);
    }
    if !current.is_empty() && lines.len() < max_lines {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push("未获取到页面描述".to_string());
    }
    if value.chars().count() > lines.iter().map(|line| line.chars().count()).sum::<usize>()
        && let Some(last) = lines.last_mut()
    {
        *last = truncate_for_card(last, max_chars.saturating_sub(1));
    }
    lines
}

fn split_wrappable(value: &str) -> Vec<String> {
    if value.chars().any(|ch| !ch.is_ascii()) {
        value.chars().map(|ch| ch.to_string()).collect()
    } else {
        value
            .split_whitespace()
            .map(|word| format!(" {word}"))
            .collect()
    }
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn non_empty<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value.trim()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_first_http_url() {
        assert_eq!(
            extract_first_url("看看 https://example.com/a?b=1。"),
            Some("https://example.com/a?b=1".to_string())
        );
    }

    #[test]
    fn parses_seo_and_media_metadata() {
        let url = Url::parse("https://example.com/news/page").unwrap();
        let metadata = parse_link_metadata(
            &url,
            r#"<html><head>
                <title>fallback</title>
                <meta property="og:title" content="页面标题">
                <meta name="description" content="页面描述">
                <meta property="og:site_name" content="示例站点">
                <meta property="og:image" content="/cover.jpg">
                <meta property="og:video" content="https://cdn.example.com/video.mp4">
            </head></html>"#,
            "text/html",
        );

        assert_eq!(metadata.title, "页面标题");
        assert_eq!(metadata.description, "页面描述");
        assert_eq!(metadata.site_name, "示例站点");
        assert_eq!(
            metadata.image_url,
            Some("https://example.com/cover.jpg".to_string())
        );
        assert_eq!(
            metadata.video_url,
            Some("https://cdn.example.com/video.mp4".to_string())
        );
    }

    #[test]
    fn round_trips_link_preview_card_query() {
        let card = LinkPreviewCard {
            url: "https://example.com".to_string(),
            host: "example.com".to_string(),
            title: "标题".to_string(),
            description: "描述".to_string(),
            site_name: "站点".to_string(),
            content_type: "text/html".to_string(),
            image_url: Some("https://example.com/a.jpg".to_string()),
            video_url: Some("https://example.com/a.mp4".to_string()),
            ssl_issuer: Some("Example CA".to_string()),
        };

        assert_eq!(
            link_preview_card_from_query(&link_preview_card_query(&card)),
            card
        );
    }

    #[test]
    fn renders_link_preview_png() {
        let card = LinkPreviewCard {
            url: "https://example.com".to_string(),
            host: "example.com".to_string(),
            title: "标题".to_string(),
            description: "描述".to_string(),
            site_name: "站点".to_string(),
            content_type: "text/html".to_string(),
            image_url: None,
            video_url: None,
            ssl_issuer: Some("Example CA".to_string()),
        };

        let png = render_link_preview_png(&card).unwrap();

        assert!(png.starts_with(b"\x89PNG"));
    }
}
