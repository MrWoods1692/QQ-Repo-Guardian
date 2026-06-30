use scraper::{Html, Selector};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProviderPricing {
    pub key: &'static str,
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub subscription: &'static str,
    pub subscription_source: &'static str,
    pub api: &'static str,
    pub source: &'static str,
    pub note: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAiProviderPricing {
    pub provider: AiProviderPricing,
    pub subscription: String,
    pub subscription_source: String,
    pub subscription_status: String,
}

pub fn all_provider_pricing() -> Vec<AiProviderPricing> {
    vec![
        AiProviderPricing {
            key: "deepseek",
            name: "DeepSeek",
            aliases: &["deepseek", "深度求索"],
            subscription: "网页/移动端常规使用；会员与增值规则以官方应用展示为准",
            subscription_source: "https://chat.deepseek.com/",
            api: "DeepSeek Chat / Reasoner 按百万 token 计费，区分输入、缓存命中和输出",
            source: "https://api-docs.deepseek.com/quick_start/pricing",
            note: "API 价格变动较快，卡片展示入口和计费口径。",
        },
        AiProviderPricing {
            key: "claude",
            name: "Claude",
            aliases: &["claude", "anthropic"],
            subscription: "Free / Pro / Max 团队套餐；个人 Pro 常见为月付档",
            subscription_source: "https://www.anthropic.com/pricing",
            api: "Claude 3/3.5/4 系列按输入、输出 token 分别计费",
            source: "https://www.anthropic.com/pricing",
            note: "订阅和 API 分开计费，企业价需联系销售。",
        },
        AiProviderPricing {
            key: "gpt",
            name: "GPT / OpenAI",
            aliases: &["gpt", "openai", "chatgpt"],
            subscription: "ChatGPT Free / Plus / Pro / Team / Enterprise",
            subscription_source: "https://openai.com/chatgpt/pricing/",
            api: "GPT、Reasoning、Embedding、Image、Audio 等模型按项目分别计费",
            source: "https://openai.com/api/pricing/",
            note: "ChatGPT 订阅不等同于 API 额度。",
        },
        AiProviderPricing {
            key: "minimax",
            name: "MiniMax",
            aliases: &["minimax", "海螺", "abab"],
            subscription: "海螺 AI 等产品会员以应用内展示为准",
            subscription_source: "https://hailuoai.com/",
            api: "文本、语音、视频、多模态模型按调用量或 token 计费",
            source: "https://platform.minimaxi.com/document/price",
            note: "国内平台通常存在套餐包与按量两套口径。",
        },
        AiProviderPricing {
            key: "mimo",
            name: "MiMo",
            aliases: &["mimo", "小米mimo", "mi mo"],
            subscription: "公开订阅价格以小米 AI 产品入口为准",
            subscription_source: "https://github.com/XiaomiMiMo/MiMo",
            api: "开放平台/API 价格以小米官方模型服务页面为准",
            source: "https://github.com/XiaomiMiMo/MiMo",
            note: "MiMo 公开资料更多偏模型与开源信息，商业价格需看官方入口。",
        },
        AiProviderPricing {
            key: "kimi",
            name: "Kimi / Moonshot",
            aliases: &["kimi", "moonshot", "月之暗面"],
            subscription: "Kimi 会员/增值服务以 Kimi 应用内展示为准",
            subscription_source: "https://kimi.moonshot.cn/",
            api: "Moonshot API 按模型上下文长度和输入/输出 token 计费",
            source: "https://platform.moonshot.cn/docs/pricing",
            note: "长上下文模型价格通常高于短上下文模型。",
        },
        AiProviderPricing {
            key: "qwen",
            name: "Qwen / 通义千问",
            aliases: &["qwen", "通义", "通义千问", "dashscope"],
            subscription: "通义千问 App / 办公产品会员以阿里云或应用展示为准",
            subscription_source: "https://tongyi.aliyun.com/",
            api: "DashScope 百炼按模型、输入输出 token、图像/语音能力计费",
            source: "https://help.aliyun.com/zh/model-studio/billing-of-model-studio",
            note: "阿里云常有免费额度、资源包和阶梯计费。",
        },
        AiProviderPricing {
            key: "glm",
            name: "GLM / 智谱",
            aliases: &["glm", "智谱", "智谱清言", "bigmodel"],
            subscription: "智谱清言会员以应用内展示为准",
            subscription_source: "https://chatglm.cn/",
            api: "BigModel 开放平台按 GLM 系列模型 token 和工具能力计费",
            source: "https://open.bigmodel.cn/pricing",
            note: "部分模型可能提供免费额度或限时优惠。",
        },
        AiProviderPricing {
            key: "gemini",
            name: "Gemini",
            aliases: &["gemini", "google", "谷歌"],
            subscription: "Google AI Pro / Ultra 等订阅，按地区显示本地价格",
            subscription_source: "https://one.google.com/about/google-ai-plans/",
            api: "Gemini API 按模型、输入输出 token、缓存、图像/音视频能力计费",
            source: "https://ai.google.dev/gemini-api/docs/pricing",
            note: "Google 订阅价格地区差异明显。",
        },
        AiProviderPricing {
            key: "grok",
            name: "Grok / xAI",
            aliases: &["grok", "xai", "x.ai"],
            subscription: "X Premium / Premium+ 或 SuperGrok 等订阅入口",
            subscription_source: "https://grok.com/",
            api: "xAI API 按模型输入、输出 token 计费",
            source: "https://docs.x.ai/docs/models",
            note: "Grok 订阅入口与 API 平台分属不同计费体系。",
        },
    ]
}

pub fn find_provider(query: &str) -> Option<AiProviderPricing> {
    let normalized = normalize_query(query);
    all_provider_pricing().into_iter().find(|provider| {
        normalized == provider.key
            || provider
                .aliases
                .iter()
                .any(|alias| normalized.contains(&normalize_query(alias)))
    })
}

pub fn parse_ai_price_query(message: &str) -> Option<Option<String>> {
    let trimmed = message.trim();
    let normalized = normalize_query(trimmed);
    if normalized == "/ai-price" || normalized == "/aiprice" || normalized == "ai价格" {
        return Some(None);
    }
    for prefix in [
        "/ai-price",
        "/aiprice",
        "/price",
        "价格",
        "查价格",
        "查询价格",
    ] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Some(find_provider(rest.trim()).map(|provider| provider.key.to_string()));
        }
    }
    if !(normalized.contains("价格") || normalized.contains("pricing")) {
        return None;
    }
    find_provider(trimmed).map(|provider| Some(provider.key.to_string()))
}

pub fn pricing_for_query(provider_key: Option<&str>) -> Vec<AiProviderPricing> {
    provider_key
        .and_then(find_provider)
        .map(|provider| vec![provider])
        .unwrap_or_else(all_provider_pricing)
}

pub async fn resolve_pricing_for_query(
    client: &reqwest::Client,
    provider_key: Option<&str>,
) -> Vec<ResolvedAiProviderPricing> {
    let providers = pricing_for_query(provider_key);
    let mut resolved = Vec::with_capacity(providers.len());
    for provider in providers {
        resolved.push(resolve_provider_pricing(client, provider).await);
    }
    resolved
}

pub async fn resolve_provider_pricing(
    client: &reqwest::Client,
    provider: AiProviderPricing,
) -> ResolvedAiProviderPricing {
    let subscription_source = provider.subscription_source.to_string();
    match fetch_subscription_pricing(client, &provider).await {
        Ok(subscription) => ResolvedAiProviderPricing {
            provider,
            subscription,
            subscription_source,
            subscription_status: "订阅价来自网页解析".to_string(),
        },
        Err(error) => ResolvedAiProviderPricing {
            subscription: format!("{}（网页解析失败：{}）", provider.subscription, error),
            provider,
            subscription_source,
            subscription_status: "订阅价解析失败，已使用兜底摘要".to_string(),
        },
    }
}

pub async fn fetch_subscription_pricing(
    client: &reqwest::Client,
    provider: &AiProviderPricing,
) -> anyhow::Result<String> {
    let html = client
        .get(provider.subscription_source)
        .header(reqwest::header::USER_AGENT, "qq-repo-guardian ai-pricing")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    parse_subscription_pricing_from_html(&html)
        .ok_or_else(|| anyhow::anyhow!("未在页面文本中识别到价格"))
}

pub fn render_ai_pricing_text(providers: &[ResolvedAiProviderPricing]) -> String {
    let title = if providers.len() == 1 {
        format!("{} 价格查询", providers[0].provider.name)
    } else {
        "AI 厂商价格查询".to_string()
    };
    let rows = providers
        .iter()
        .map(|provider| {
            format!(
                "{}\n订阅：{}\n订阅来源：{}\nAPI：{}\nAPI来源：{}",
                provider.provider.name,
                provider.subscription,
                provider.subscription_source,
                provider.provider.api,
                provider.provider.source
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("{title}\n{rows}\n\n价格会变动，以官方页面实时展示为准。")
}

pub fn render_ai_pricing_png(providers: &[ResolvedAiProviderPricing]) -> anyhow::Result<Vec<u8>> {
    crate::github::svg_to_png(&render_ai_pricing_svg(providers))
}

pub fn render_ai_pricing_svg(providers: &[ResolvedAiProviderPricing]) -> String {
    let title = if providers.len() == 1 {
        format!("{} 价格查询", providers[0].provider.name)
    } else {
        "AI 厂商价格查询".to_string()
    };
    let subtitle = if providers.len() == 1 {
        "订阅与 API 价格摘要".to_string()
    } else {
        "DeepSeek / Claude / GPT / MiniMax / MiMo / Kimi / Qwen / GLM / Gemini / Grok".to_string()
    };
    let rows = render_provider_rows(providers);
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="980" height="760" viewBox="0 0 980 760">
  <defs>
    <linearGradient id="aiBg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#0f766e"/>
      <stop offset="0.45" stop-color="#2563eb"/>
      <stop offset="1" stop-color="#be123c"/>
    </linearGradient>
    <filter id="aiShadow" x="-10%" y="-10%" width="120%" height="130%">
      <feDropShadow dx="0" dy="18" stdDeviation="18" flood-color="#0f172a" flood-opacity="0.24"/>
    </filter>
  </defs>
  <rect width="980" height="760" rx="30" fill="url(#aiBg)"/>
  <path d="M0 138 C116 54 226 56 344 106 C474 160 560 104 688 52 C804 4 902 34 980 96 L980 0 L0 0Z" fill="#ffffff" opacity="0.15"/>
  <rect x="42" y="42" width="896" height="676" rx="24" fill="#ffffff" opacity="0.97" filter="url(#aiShadow)"/>
  <rect x="42" y="42" width="896" height="7" rx="3" fill="#22c55e"/>
  <rect x="70" y="72" width="150" height="38" rx="19" fill="#ecfeff"/>
  <circle cx="95" cy="91" r="10" fill="#0891b2"/>
  <text x="116" y="98" fill="#0e7490" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="16" font-weight="850">价格查询</text>
  <text x="70" y="154" fill="#0f172a" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="36" font-weight="850">{}</text>
  <text x="70" y="188" fill="#475569" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="18" font-weight="650">{}</text>
  <rect x="70" y="214" width="840" height="44" rx="14" fill="#f8fafc"/>
  <text x="94" y="242" fill="#334155" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="15" font-weight="750">订阅价与 API 价分属不同体系；部分厂商按地区、额度包、模型版本实时变化。</text>
  {}
  <text x="70" y="692" fill="#64748b" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="14" font-weight="700">数据口径：公开官网/文档入口摘要，最终金额以官方实时页面为准。</text>
</svg>"##,
        escape_html(&title),
        escape_html(&subtitle),
        rows,
    )
}

fn render_provider_rows(providers: &[ResolvedAiProviderPricing]) -> String {
    let visible = providers.iter().take(10).collect::<Vec<_>>();
    let row_height = if visible.len() <= 1 { 250 } else { 40 };
    let start_y = if visible.len() <= 1 { 292 } else { 286 };
    visible
        .iter()
        .enumerate()
        .map(|(index, provider)| {
            let y = start_y + index as i32 * row_height;
            if visible.len() == 1 {
                render_single_provider(provider, y)
            } else {
                render_compact_provider(provider, y, index)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_single_provider(provider: &ResolvedAiProviderPricing, y: i32) -> String {
    let subscription = svg_text_lines(&provider.subscription, 42, 3, 114, y + 86, 24);
    let api = svg_text_lines(provider.provider.api, 42, 3, 522, y + 86, 24);
    format!(
        r##"<rect x="70" y="{}" width="840" height="250" rx="18" fill="#f8fafc"/>
  <text x="104" y="{}" fill="#0f172a" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="30" font-weight="850">{}</text>
  <text x="104" y="{}" fill="#2563eb" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="16" font-weight="750">{}</text>
  <text x="104" y="{}" fill="#1d4ed8" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="15" font-weight="850">订阅价格</text>
  <text fill="#334155" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="18" font-weight="650">{}</text>
  <text x="512" y="{}" fill="#15803d" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="15" font-weight="850">API 价格</text>
  <text fill="#334155" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="18" font-weight="650">{}</text>
  <text x="104" y="{}" fill="#64748b" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="15" font-weight="700">{}</text>"##,
        y,
        y + 42,
        escape_html(provider.provider.name),
        y + 68,
        escape_html(&format!(
            "{} · {}",
            provider.subscription_status, provider.subscription_source
        )),
        y + 106,
        subscription,
        y + 106,
        api,
        y + 222,
        escape_html(provider.provider.note),
    )
}

fn render_compact_provider(provider: &ResolvedAiProviderPricing, y: i32, index: usize) -> String {
    let fill = if index % 2 == 0 { "#f8fafc" } else { "#f1f5f9" };
    format!(
        r##"<rect x="70" y="{}" width="840" height="34" rx="10" fill="{}"/>
  <text x="92" y="{}" fill="#0f172a" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="17" font-weight="850">{}</text>
  <text x="230" y="{}" fill="#334155" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="14" font-weight="700">订阅：{}</text>
  <text x="560" y="{}" fill="#334155" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="14" font-weight="700">API：{}</text>"##,
        y,
        fill,
        y + 23,
        escape_html(provider.provider.name),
        y + 23,
        escape_html(&truncate_for_card(&provider.subscription, 30)),
        y + 23,
        escape_html(&truncate_for_card(provider.provider.api, 31)),
    )
}

pub fn parse_subscription_pricing_from_html(html: &str) -> Option<String> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("body").ok()?;
    let body_text = document
        .select(&selector)
        .next()
        .map(|body| body.text().collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| document.root_element().text().collect::<Vec<_>>().join(" "));
    extract_subscription_price_lines(&body_text)
}

fn extract_subscription_price_lines(text: &str) -> Option<String> {
    let normalized = text
        .replace('\u{a0}', " ")
        .replace('\n', " ")
        .replace('\r', " ")
        .replace('\t', " ");
    let tokens = normalized
        .split(|ch: char| matches!(ch, '|' | '。' | '；' | ';' | '•' | '·'))
        .flat_map(|part| split_price_windows(part, 96))
        .filter_map(|part| normalize_price_line(&part))
        .filter(|line| looks_like_subscription_price(line))
        .fold(Vec::<String>::new(), |mut lines, line| {
            if !lines.iter().any(|existing| existing == &line) {
                lines.push(line);
            }
            lines
        });

    if tokens.is_empty() {
        None
    } else {
        Some(tokens.into_iter().take(6).collect::<Vec<_>>().join("；"))
    }
}

fn split_price_windows(value: &str, max_chars: usize) -> Vec<String> {
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return vec![value.to_string()];
    }

    chars
        .iter()
        .enumerate()
        .filter_map(|(index, ch)| {
            if !is_price_marker(*ch) {
                return None;
            }
            let start = index.saturating_sub(36);
            let end = (index + 60).min(chars.len());
            Some(chars[start..end].iter().collect::<String>())
        })
        .collect()
}

fn normalize_price_line(value: &str) -> Option<String> {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed
        .trim_matches(|ch: char| matches!(ch, '-' | ':' | '：' | ',' | '，' | '/' | '\\'))
        .trim();
    if trimmed.is_empty() || trimmed.chars().count() < 3 {
        None
    } else {
        Some(truncate_for_card(trimmed, 86))
    }
}

fn looks_like_subscription_price(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let has_price = value.chars().any(is_price_marker)
        || lower.contains("usd")
        || lower.contains("cny")
        || lower.contains("rmb");
    let has_plan = [
        "free",
        "plus",
        "pro",
        "max",
        "team",
        "enterprise",
        "premium",
        "ultra",
        "starter",
        "standard",
        "month",
        "monthly",
        "/mo",
        "year",
        "plan",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword))
        || ["月", "年", "会员", "套餐", "订阅", "专业", "团队", "企业"]
            .iter()
            .any(|keyword| value.contains(keyword));

    has_price && has_plan
}

fn is_price_marker(ch: char) -> bool {
    matches!(ch, '$' | '¥' | '￥' | '€' | '£')
}

fn normalize_query(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .replace(' ', "")
        .replace('：', ":")
        .replace("查询", "")
        .replace("多少", "")
        .replace("多少钱", "")
}

fn svg_text_lines(
    value: &str,
    max_chars: usize,
    max_lines: usize,
    x: i32,
    y: i32,
    line_height: i32,
) -> String {
    wrap_text(value, max_chars, max_lines)
        .iter()
        .enumerate()
        .map(|(index, line)| {
            format!(
                r##"<tspan x="{}" y="{}">{}</tspan>"##,
                x,
                y + index as i32 * line_height,
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
        lines.push("以官方页面为准".to_string());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ai_price_queries() {
        assert_eq!(parse_ai_price_query("/ai-price"), Some(None));
        assert_eq!(
            parse_ai_price_query("/ai-price deepseek"),
            Some(Some("deepseek".to_string()))
        );
        assert_eq!(
            parse_ai_price_query("gpt价格查询"),
            Some(Some("gpt".to_string()))
        );
        assert_eq!(parse_ai_price_query("随便聊聊"), None);
    }

    #[test]
    fn renders_text_for_single_provider() {
        let providers = vec![ResolvedAiProviderPricing {
            provider: find_provider("claude").unwrap(),
            subscription: "Pro $20/month；Max $100/month".to_string(),
            subscription_source: "https://www.anthropic.com/pricing".to_string(),
            subscription_status: "订阅价来自网页解析".to_string(),
        }];
        let text = render_ai_pricing_text(&providers);

        assert!(text.contains("Claude 价格查询"));
        assert!(text.contains("Pro $20/month"));
        assert!(text.contains("API："));
    }

    #[test]
    fn renders_ai_pricing_png() {
        let providers = vec![ResolvedAiProviderPricing {
            provider: find_provider("deepseek").unwrap(),
            subscription: "会员 ¥30/月".to_string(),
            subscription_source: "https://chat.deepseek.com/".to_string(),
            subscription_status: "订阅价来自网页解析".to_string(),
        }];
        let png = render_ai_pricing_png(&providers).unwrap();

        assert!(png.starts_with(b"\x89PNG"));
    }

    #[test]
    fn parses_subscription_prices_from_html() {
        let html = r#"
            <main>
                <section>Free $0/month</section>
                <section>Plus $20/month billed monthly</section>
                <section>Team $25 per user / month</section>
            </main>
        "#;

        let pricing = parse_subscription_pricing_from_html(html).unwrap();

        assert!(pricing.contains("Free $0/month"));
        assert!(pricing.contains("Plus $20/month"));
        assert!(pricing.contains("Team $25"));
    }
}
