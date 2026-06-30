use serde::Deserialize;

const NEWS_API_URL: &str = "https://yunzhiapi.cn/API/mraikx.php";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyNewsDigest {
    pub date: String,
    pub items: Vec<DailyNewsItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyNewsItem {
    pub title: String,
    pub link: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    status: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    data: Vec<ApiDateNews>,
}

#[derive(Debug, Deserialize)]
struct ApiDateNews {
    #[serde(default, alias = "datetime")]
    date: String,
    #[serde(default)]
    news: Vec<ApiNewsItem>,
}

#[derive(Debug, Deserialize)]
struct ApiNewsItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    link: String,
    #[serde(default)]
    content: String,
}

pub async fn fetch_daily_news(
    client: &reqwest::Client,
    token: &str,
) -> anyhow::Result<DailyNewsDigest> {
    let response = client
        .get(NEWS_API_URL)
        .query(&[("token", token), ("type", "json")])
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let response = serde_json::from_str::<ApiResponse>(&response)?;

    parse_api_response(response)
}

fn parse_api_response(response: ApiResponse) -> anyhow::Result<DailyNewsDigest> {
    if !response.status.eq_ignore_ascii_case("success") {
        anyhow::bail!("快讯接口返回失败：{}", response.message);
    }

    let Some(day) = response.data.into_iter().find(|day| !day.news.is_empty()) else {
        anyhow::bail!("快讯接口未返回新闻内容");
    };

    let items = day
        .news
        .into_iter()
        .filter_map(|item| {
            let title = item.title.trim();
            if title.is_empty() {
                return None;
            }
            Some(DailyNewsItem {
                title: title.to_string(),
                link: item.link.trim().to_string(),
                content: item.content.trim().to_string(),
            })
        })
        .collect::<Vec<_>>();

    if items.is_empty() {
        anyhow::bail!("快讯接口新闻标题为空");
    }

    Ok(DailyNewsDigest {
        date: day.date.trim().to_string(),
        items,
    })
}

pub fn render_daily_news_message(digest: &DailyNewsDigest) -> String {
    let header = if digest.date.is_empty() {
        "AI 快讯".to_string()
    } else {
        format!("AI 快讯 · {}", digest.date)
    };
    let rows = digest
        .items
        .iter()
        .take(6)
        .enumerate()
        .map(|(index, item)| {
            let mut row = format!("{}. {}", index + 1, item.title);
            if !item.content.is_empty() {
                row.push_str(&format!("\n{}", truncate_text(&item.content, 120)));
            }
            if !item.link.is_empty() {
                row.push_str(&format!("\n{}", item.link));
            }
            row
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    format!("{header}\n\n{rows}")
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    let mut text = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        text.push('…');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_daily_news_message() {
        let digest = DailyNewsDigest {
            date: "11月18·周二".to_string(),
            items: vec![DailyNewsItem {
                title: "Grok 4.1 发布".to_string(),
                link: "https://example.com/news".to_string(),
                content: "模型能力提升".to_string(),
            }],
        };

        let message = render_daily_news_message(&digest);

        assert!(message.contains("AI 快讯 · 11月18·周二"));
        assert!(message.contains("1. Grok 4.1 发布"));
        assert!(message.contains("https://example.com/news"));
    }
}
