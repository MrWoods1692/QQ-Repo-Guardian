use serde::Deserialize;

const WEATHER_API_URL: &str = "https://yunzhiapi.cn/API/zxtqsk.php";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeatherSnapshot {
    pub city: String,
    pub path: String,
    pub condition: String,
    pub condition_code: String,
    pub temperature: String,
    pub last_update: String,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    status: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    data: Option<ApiData>,
}

#[derive(Debug, Deserialize)]
struct ApiData {
    #[serde(default)]
    results: Vec<ApiResult>,
}

#[derive(Debug, Deserialize)]
struct ApiResult {
    #[serde(default)]
    location: ApiLocation,
    #[serde(default)]
    now: ApiNow,
    #[serde(default)]
    last_update: String,
}

#[derive(Debug, Deserialize, Default)]
struct ApiLocation {
    #[serde(default)]
    name: String,
    #[serde(default)]
    path: String,
}

#[derive(Debug, Deserialize, Default)]
struct ApiNow {
    #[serde(default)]
    text: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    temperature: String,
}

pub async fn fetch_weather(
    client: &reqwest::Client,
    token: &str,
    location: &str,
) -> anyhow::Result<WeatherSnapshot> {
    let response = client
        .get(WEATHER_API_URL)
        .query(&[("token", token), ("location", location), ("type", "json")])
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let response = serde_json::from_str::<ApiResponse>(&response)?;
    parse_weather_response(response)
}

fn parse_weather_response(response: ApiResponse) -> anyhow::Result<WeatherSnapshot> {
    if !response.status.eq_ignore_ascii_case("success") {
        anyhow::bail!("天气接口返回失败：{}", response.message);
    }

    let data = response
        .data
        .ok_or_else(|| anyhow::anyhow!("天气接口未返回数据"))?;
    let result = data
        .results
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("天气接口未返回城市结果"))?;

    Ok(WeatherSnapshot {
        city: result.location.name.trim().to_string(),
        path: result.location.path.trim().to_string(),
        condition: result.now.text.trim().to_string(),
        condition_code: result.now.code.trim().to_string(),
        temperature: result.now.temperature.trim().to_string(),
        last_update: result.last_update.trim().to_string(),
    })
}

pub fn render_weather_message(snapshot: &WeatherSnapshot) -> String {
    let emoji = weather_emoji(&snapshot.condition_code, &snapshot.condition);
    format!(
        "今日天气 · {}\n\n{emoji} {}  {}°C\n地区：{}\n更新时间：{}",
        snapshot.city,
        snapshot.condition,
        snapshot.temperature,
        snapshot.path,
        snapshot.last_update
    )
}

pub fn weather_snapshot_from_query(encoded: &str) -> WeatherSnapshot {
    let params: std::collections::HashMap<String, String> =
        url::form_urlencoded::parse(encoded.as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
    WeatherSnapshot {
        city: params.get("city").cloned().unwrap_or_default(),
        path: params.get("path").cloned().unwrap_or_default(),
        condition: params.get("condition").cloned().unwrap_or_default(),
        condition_code: params.get("code").cloned().unwrap_or_default(),
        temperature: params.get("temp").cloned().unwrap_or_default(),
        last_update: params.get("update").cloned().unwrap_or_default(),
    }
}

pub fn weather_card_query(snapshot: &WeatherSnapshot) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("city", &snapshot.city)
        .append_pair("path", &snapshot.path)
        .append_pair("condition", &snapshot.condition)
        .append_pair("code", &snapshot.condition_code)
        .append_pair("temp", &snapshot.temperature)
        .append_pair("update", &snapshot.last_update)
        .finish()
}

pub fn render_weather_png(snapshot: &WeatherSnapshot) -> anyhow::Result<Vec<u8>> {
    crate::github::svg_to_png(&render_weather_svg(snapshot))
}

pub fn render_weather_svg(snapshot: &WeatherSnapshot) -> String {
    let emoji = weather_emoji(&snapshot.condition_code, &snapshot.condition);
    let city = escape_html(&snapshot.city);
    let cond = escape_html(&snapshot.condition);
    let temp = escape_html(&format!("{}°C", snapshot.temperature));
    let path = escape_html(&snapshot.path);
    let update = escape_html(&snapshot.last_update);
    let update_short = if update.len() > 16 {
        &update[..16]
    } else {
        &update
    };

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="600" height="340" viewBox="0 0 600 340">
  <defs>
    <linearGradient id="wBg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#0284c7"/>
      <stop offset="0.5" stop-color="#0891b2"/>
      <stop offset="1" stop-color="#06b6d4"/>
    </linearGradient>
    <filter id="wShadow" x="-10%" y="-10%" width="120%" height="130%">
      <feDropShadow dx="0" dy="14" stdDeviation="14" flood-color="#0f172a" flood-opacity="0.2"/>
    </filter>
  </defs>
  <rect width="600" height="340" rx="24" fill="url(#wBg)"/>
  <rect x="28" y="28" width="544" height="284" rx="20" fill="#ffffff" opacity="0.97" filter="url(#wShadow)"/>
  <rect x="28" y="28" width="544" height="6" rx="3" fill="#0284c7"/>
  <!-- 标题 -->
  <rect x="52" y="56" width="120" height="34" rx="17" fill="#ecfeff"/>
  <text x="78" y="79" fill="#0e7490" font-family="Noto Sans CJK SC, Inter, Arial, sans-serif" font-size="15" font-weight="850">今日天气</text>
  <text x="188" y="80" fill="#0f172a" font-family="Noto Sans CJK SC, Inter, Arial, sans-serif" font-size="28" font-weight="850">{city}</text>
  <!-- 天气图标 -->
  <text x="52" y="156" font-size="64">{emoji}</text>
  <!-- 天气状况 -->
  <text x="138" y="138" fill="#0f172a" font-family="Noto Sans CJK SC, Inter, Arial, sans-serif" font-size="36" font-weight="850">{cond}</text>
  <!-- 温度 -->
  <text x="138" y="180" fill="#0284c7" font-family="Inter, Arial, sans-serif" font-size="32" font-weight="850">{temp}</text>
  <!-- 地区 -->
  <rect x="52" y="216" width="496" height="36" rx="12" fill="#f0f9ff"/>
  <text x="72" y="240" fill="#334155" font-family="Noto Sans CJK SC, Inter, Arial, sans-serif" font-size="16" font-weight="700">📍 {path}</text>
  <!-- 更新时间 -->
  <text x="52" y="294" fill="#94a3b8" font-family="Noto Sans CJK SC, Inter, Arial, sans-serif" font-size="13" font-weight="650">数据更新：{update_short}</text>
</svg>"##,
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn weather_emoji(code: &str, text: &str) -> &'static str {
    let code: u32 = code.parse().unwrap_or(99);
    match code {
        0 | 1 | 2 | 3 => match () {
            _ if text.contains("晴") => "☀️",
            _ => "🌤️",
        },
        4 | 5 | 6 | 7 | 8 => "☁️",
        9 => "🌧️",
        10 | 11 | 12 => "🌦️",
        13 | 14 | 15 | 16 | 17 => "🌨️",
        18 => "🌫️",
        19 | 20 | 21 | 22 | 23 | 24 | 25 => "💨",
        26 | 27 | 28 | 29 => "🌪️",
        30 | 31 | 32 | 33 | 34 | 35 | 36 => "🌫️",
        37 | 38 => "⛈️",
        _ => {
            let lower = text.to_ascii_lowercase();
            if lower.contains("晴") {
                "☀️"
            } else if lower.contains("云") {
                "☁️"
            } else if lower.contains("雨") {
                "🌧️"
            } else if lower.contains("雪") {
                "🌨️"
            } else if lower.contains("雾") || lower.contains("霾") {
                "🌫️"
            } else if lower.contains("风") {
                "💨"
            } else {
                "🌈"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_weather_response() {
        let response = ApiResponse {
            status: "success".to_string(),
            message: String::new(),
            data: Some(ApiData {
                results: vec![ApiResult {
                    location: ApiLocation {
                        name: "长沙".to_string(),
                        path: "长沙,长沙,湖南,中国".to_string(),
                    },
                    now: ApiNow {
                        text: "雾".to_string(),
                        code: "30".to_string(),
                        temperature: "3".to_string(),
                    },
                    last_update: "2026-01-02T21:17:37+08:00".to_string(),
                }],
            }),
        };

        let snapshot = parse_weather_response(response).unwrap();
        assert_eq!(snapshot.city, "长沙");
        assert_eq!(snapshot.condition, "雾");
        assert_eq!(snapshot.temperature, "3");
    }

    #[test]
    fn renders_weather_png() {
        let snapshot = WeatherSnapshot {
            city: "长沙".to_string(),
            path: "长沙,长沙,湖南,中国".to_string(),
            condition: "雾".to_string(),
            condition_code: "30".to_string(),
            temperature: "3".to_string(),
            last_update: "2026-01-02T21:17:37+08:00".to_string(),
        };

        let png = render_weather_png(&snapshot).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
    }

    #[test]
    fn round_trips_weather_card_query() {
        let snapshot = WeatherSnapshot {
            city: "北京".to_string(),
            path: "北京,北京,中国".to_string(),
            condition: "晴".to_string(),
            condition_code: "0".to_string(),
            temperature: "25".to_string(),
            last_update: "2026-06-30T08:00:00+08:00".to_string(),
        };

        let query = weather_card_query(&snapshot);
        let roundtripped = weather_snapshot_from_query(&query);
        assert_eq!(roundtripped.city, "北京");
        assert_eq!(roundtripped.condition, "晴");
        assert_eq!(roundtripped.temperature, "25");
    }
}
