const DEEPSEEK_API_URL: &str = "https://yunzhiapi.cn/API/deepseek.php";

/// 调用 DeepSeek-R1 API 获取回复。
/// 返回纯文本回复（已去除 &lt;think&gt;...&lt;/think&gt; 推理过程）。
pub async fn ask_deepseek(
    client: &reqwest::Client,
    token: &str,
    question: &str,
) -> anyhow::Result<String> {
    let response = client
        .get(DEEPSEEK_API_URL)
        .query(&[("token", token), ("question", question), ("type", "text")])
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    Ok(strip_thinking(&response))
}

/// 去除 &lt;think&gt;...&lt;/think&gt; 块及其中的内容，返回纯回答文本。
fn strip_thinking(raw: &str) -> String {
    let raw = raw.trim();
    // 如果以 <think> 开头，找到 </think> 闭合标签
    if let Some(rest) = raw.strip_prefix("<think>") {
        if let Some(end) = rest.find("</think>") {
            return rest[end + "</think>".len()..].trim().to_string();
        }
        // 没有闭合标签，返回空（整个都是思考过程）
    }
    // 可能 <think> 不在开头
    if let Some(start) = raw.find("<think>") {
        let before = &raw[..start];
        let after_think = &raw[start + "<think>".len()..];
        if let Some(end) = after_think.find("</think>") {
            let after = after_think[end + "</think>".len()..].trim();
            return format!("{} {}", before.trim(), after).trim().to_string();
        }
        // 没有闭合标签，返回 <think> 之前的内容
        return before.trim().to_string();
    }
    raw.to_string()
}

/// 检查消息是否是一个群聊 /chat 命令。
/// 返回剥离前缀后的提问内容，如果只是 "/chat" 则返回空字符串。
pub fn parse_chat_command(message: &str) -> Option<String> {
    let trimmed = message.trim();
    if trimmed == "/chat" {
        return Some(String::new());
    }
    trimmed.strip_prefix("/chat ").map(|q| q.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_chat_with_question() {
        assert_eq!(parse_chat_command("/chat 你好"), Some("你好".to_string()));
    }

    #[test]
    fn parse_chat_empty() {
        assert_eq!(parse_chat_command("/chat"), Some(String::new()));
    }

    #[test]
    fn parse_chat_with_extra_spaces() {
        assert_eq!(
            parse_chat_command("/chat   今天天气怎么样   "),
            Some("今天天气怎么样".to_string())
        );
    }

    #[test]
    fn parse_chat_not_chat() {
        assert_eq!(parse_chat_command("你好"), None);
        assert_eq!(parse_chat_command("/repo-guardian ping"), None);
    }

    #[test]
    fn strip_thinking_no_think() {
        assert_eq!(
            strip_thinking("你好，我是DeepSeek。"),
            "你好，我是DeepSeek。"
        );
    }

    #[test]
    fn strip_thinking_with_think_block() {
        let raw = "<think>\n用户问了一个问题\n</think>\n你好呀！我是 DeepSeek-R1";
        assert_eq!(strip_thinking(raw), "你好呀！我是 DeepSeek-R1");
    }

    #[test]
    fn strip_thinking_multiline_think() {
        let raw = "<think>\n嗯，用户问了一个非常基础的问题。\n这通常是用户第一次接触AI助手。\n</think>\n你好呀！";
        assert_eq!(strip_thinking(raw), "你好呀！");
    }

    #[test]
    fn strip_thinking_only_think() {
        let raw = "<think>思考中</think>";
        assert_eq!(strip_thinking(raw), "");
    }
}
