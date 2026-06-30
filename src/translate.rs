use serde::Deserialize;

const TRANSLATE_API_URL: &str = "https://yunzhiapi.cn/API/wnyyfy.php";

#[derive(Debug, Deserialize)]
struct ApiResponse {
    status: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    data: Option<ApiData>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ApiData {
    #[serde(default, alias = "original_text")]
    original_text: String,
    #[serde(default, alias = "target_language")]
    target_language: String,
    #[serde(default, alias = "translated_text")]
    translated_text: String,
}

pub async fn translate_to_chinese(
    client: &reqwest::Client,
    token: &str,
    text: &str,
) -> anyhow::Result<String> {
    let response = client
        .get(TRANSLATE_API_URL)
        .query(&[
            ("token", token),
            ("msg", text),
            ("target", "zh-cn"),
            ("type", "json"),
        ])
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let response = serde_json::from_str::<ApiResponse>(&response)?;
    if !response.status.eq_ignore_ascii_case("success") {
        anyhow::bail!("翻译接口返回失败：{}", response.message);
    }
    let data = response
        .data
        .ok_or_else(|| anyhow::anyhow!("翻译接口未返回数据"))?;
    if data.translated_text.trim().is_empty() {
        anyhow::bail!("翻译结果为空");
    }
    Ok(data.translated_text.trim().to_string())
}

/// 检测消息是否包含非中文内容（需要翻译）。
/// 返回 None 表示不需要翻译。
/// 返回 Some 表示含有值得翻译的外文内容。
pub fn needs_translation(message: &str) -> Option<String> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 去掉 CQ 码
    let plain = strip_cq_codes(trimmed);
    if plain.is_empty() {
        return None;
    }

    // 含链接不翻译
    if plain.contains("http://") || plain.contains("https://") {
        return None;
    }

    // 带中文的不翻译
    if plain.chars().any(|ch| is_cjk(ch)) {
        return None;
    }

    let total = plain.chars().count();
    if total < 4 {
        return None;
    }

    // 存在连续 3 个以上拉丁字母的单词才翻译
    if has_foreign_words(&plain) {
        return Some(plain);
    }
    None
}

fn strip_cq_codes(message: &str) -> String {
    let mut result = String::new();
    let mut in_cq = false;
    for ch in message.chars() {
        if ch == '[' {
            in_cq = true;
            continue;
        }
        if in_cq {
            if ch == ']' {
                in_cq = false;
            }
            continue;
        }
        result.push(ch);
    }
    result
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified
        | '\u{3400}'..='\u{4DBF}'   // CJK Extension A
        | '\u{F900}'..='\u{FAFF}'   // CJK Compatibility
    )
}

fn has_foreign_words(text: &str) -> bool {
    let mut latin_run = 0u32;
    for ch in text.chars() {
        if ch.is_ascii_alphabetic() {
            latin_run += 1;
            if latin_run >= 3 {
                return true;
            }
        } else {
            latin_run = 0;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_pure_chinese() {
        assert!(needs_translation("今天天气真好").is_none());
        assert!(needs_translation("你好，请问在吗？").is_none());
    }

    #[test]
    fn detects_english() {
        assert!(needs_translation("Hello, how are you?").is_some());
        assert!(needs_translation("This is a test message").is_some());
    }

    #[test]
    fn detects_mixed() {
        // 带中文的不翻译
        assert!(needs_translation("我今天学习了 machine learning 的基础知识").is_none());
    }

    #[test]
    fn skips_url() {
        assert!(needs_translation("check this out https://example.com/foo").is_none());
        assert!(needs_translation("see http://a.b/c").is_none());
    }

    #[test]
    fn skips_chinese() {
        // 只要包含汉字就不翻译
        assert!(needs_translation("hello 你好").is_none());
        assert!(needs_translation("中文 test").is_none());
    }

    #[test]
    fn skips_numbers_and_punctuation() {
        assert!(needs_translation("12345").is_none());
        assert!(needs_translation("!!!").is_none());
    }

    #[test]
    fn skips_short_english() {
        // "ok" 只有 2 个字母，不触发
        assert!(needs_translation("ok").is_none());
        // "hi" 同理
        assert!(needs_translation("hi").is_none());
    }
}
