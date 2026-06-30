use base64::Engine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberCardKind {
    Join,
    Leave,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberCard {
    pub kind: MemberCardKind,
    pub group_id: i64,
    pub user_id: i64,
    pub operator_id: Option<i64>,
    pub sub_type: Option<String>,
    pub nickname: Option<String>,
    pub card: Option<String>,
    pub level: Option<String>,
    pub title: Option<String>,
    pub avatar_data_uri: Option<String>,
}

pub fn member_card_query(card: &MemberCard) -> String {
    let kind = match card.kind {
        MemberCardKind::Join => "join",
        MemberCardKind::Leave => "leave",
    };
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer
        .append_pair("kind", kind)
        .append_pair("group", &card.group_id.to_string())
        .append_pair("user", &card.user_id.to_string());
    if let Some(operator_id) = card.operator_id {
        serializer.append_pair("operator", &operator_id.to_string());
    }
    if let Some(sub_type) = &card.sub_type {
        serializer.append_pair("sub_type", sub_type);
    }
    if let Some(nickname) = &card.nickname {
        serializer.append_pair("nickname", nickname);
    }
    if let Some(card_name) = &card.card {
        serializer.append_pair("card", card_name);
    }
    if let Some(level) = &card.level {
        serializer.append_pair("level", level);
    }
    if let Some(title) = &card.title {
        serializer.append_pair("title", title);
    }
    serializer.finish()
}

pub fn member_card_from_query(query: &str) -> MemberCard {
    let pairs = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect::<std::collections::HashMap<_, _>>();
    let kind = match pairs.get("kind").map(String::as_str) {
        Some("leave") => MemberCardKind::Leave,
        _ => MemberCardKind::Join,
    };

    MemberCard {
        kind,
        group_id: parse_i64(pairs.get("group")),
        user_id: parse_i64(pairs.get("user")),
        operator_id: pairs.get("operator").and_then(|value| value.parse().ok()),
        sub_type: pairs.get("sub_type").cloned(),
        nickname: pairs.get("nickname").cloned(),
        card: pairs.get("card").cloned(),
        level: pairs.get("level").cloned(),
        title: pairs.get("title").cloned(),
        avatar_data_uri: None,
    }
}

pub async fn hydrate_member_card_avatar(
    client: &reqwest::Client,
    card: &mut MemberCard,
) -> anyhow::Result<()> {
    let url = format!("https://q1.qlogo.cn/g?b=qq&nk={}&s=100", card.user_id);
    let response = client.get(url).send().await?.error_for_status()?;
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("image/jpeg")
        .split(';')
        .next()
        .unwrap_or("image/jpeg")
        .to_string();
    let bytes = response.bytes().await?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    card.avatar_data_uri = Some(format!("data:{content_type};base64,{encoded}"));
    Ok(())
}

pub fn render_member_card_png(card: &MemberCard) -> anyhow::Result<Vec<u8>> {
    crate::github::svg_to_png(&render_member_card_svg(card))
}

pub fn render_member_card_svg(card: &MemberCard) -> String {
    let (title, status, accent, soft, label) = match card.kind {
        MemberCardKind::Join => (
            "欢迎新成员入群",
            "新的连接已经建立",
            "#16a34a",
            "#dcfce7",
            "JOIN",
        ),
        MemberCardKind::Leave => (
            "成员已离开群聊",
            "群成员状态有变化",
            "#dc2626",
            "#fee2e2",
            "LEFT",
        ),
    };
    let operator = card
        .operator_id
        .map(|value| value.to_string())
        .unwrap_or_else(|| "系统事件".to_string());
    let sub_type = card
        .sub_type
        .as_deref()
        .map(member_sub_type_label)
        .unwrap_or("群事件");
    let user_suffix = card.user_id.rem_euclid(100).to_string();
    let display_name = card
        .card
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or(card.nickname.as_deref())
        .unwrap_or("QQ 用户");
    let nickname = card.nickname.as_deref().unwrap_or("未获取");
    let subtitle = format!("{} · {}", title, status);
    let level = card.level.as_deref().unwrap_or("未知");
    let title_text = card.title.as_deref().unwrap_or("暂无头衔");
    let avatar = card.avatar_data_uri.as_deref();
    let avatar_markup = avatar
        .map(|uri| {
            format!(
                r##"<clipPath id="avatarClip"><circle cx="138" cy="188" r="54"/></clipPath>
  <image href="{}" x="84" y="134" width="108" height="108" clip-path="url(#avatarClip)" preserveAspectRatio="xMidYMid slice"/>
  <circle cx="138" cy="188" r="56" fill="none" stroke="{}" stroke-width="4"/>"##,
                escape_html(uri),
                accent
            )
        })
        .unwrap_or_else(|| {
            format!(
                r##"<circle cx="138" cy="188" r="54" fill="{}"/>
  <text x="138" y="203" text-anchor="middle" fill="{}" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="34" font-weight="900">{}</text>"##,
                soft,
                accent,
                escape_html(&user_suffix)
            )
        });

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="760" height="360" viewBox="0 0 760 360">
  <defs>
    <linearGradient id="memberBg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#0f766e"/>
      <stop offset="0.52" stop-color="#2563eb"/>
      <stop offset="1" stop-color="#7c3aed"/>
    </linearGradient>
    <filter id="memberShadow" x="-10%" y="-10%" width="120%" height="130%">
      <feDropShadow dx="0" dy="16" stdDeviation="16" flood-color="#0f172a" flood-opacity="0.24"/>
    </filter>
  </defs>
  <rect width="760" height="360" rx="28" fill="url(#memberBg)"/>
  <path d="M0 92 C96 34 174 34 278 82 C388 132 484 116 588 52 C650 14 710 18 760 42 L760 0 L0 0Z" fill="#ffffff" opacity="0.16"/>
  <rect x="38" y="38" width="684" height="284" rx="22" fill="#ffffff" opacity="0.97" filter="url(#memberShadow)"/>
  <rect x="38" y="38" width="684" height="7" rx="3" fill="{}"/>
  <rect x="70" y="72" width="124" height="36" rx="18" fill="{}"/>
  <text x="98" y="96" fill="{}" font-family="Inter, Segoe UI, Arial, sans-serif" font-size="16" font-weight="850">{}</text>
    {}
  <text x="222" y="132" fill="#0f172a" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="32" font-weight="850">{}</text>
  <text x="222" y="166" fill="#475569" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="18" font-weight="650">{}</text>
  <rect x="222" y="198" width="430" height="1" fill="#e2e8f0"/>
    <text x="222" y="230" fill="#334155" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="17" font-weight="750">QQ 号：{}</text>
    <text x="222" y="258" fill="#334155" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="17" font-weight="750">昵称：{}</text>
    <text x="222" y="286" fill="#334155" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="17" font-weight="750">等级：{}</text>
    <text x="438" y="230" fill="#64748b" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="16" font-weight="650">类型：{}</text>
    <text x="438" y="258" fill="#64748b" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="16" font-weight="650">头衔：{}</text>
    <text x="438" y="286" fill="#64748b" font-family="Noto Sans CJK SC, Inter, Segoe UI, Arial, sans-serif" font-size="16" font-weight="650">操作者：{}</text>
</svg>"##,
        accent,
        soft,
        accent,
        label,
        avatar_markup,
        escape_html(display_name),
        escape_html(&subtitle),
        card.user_id,
        escape_html(nickname),
        escape_html(level),
        escape_html(sub_type),
        escape_html(title_text),
        escape_html(&operator),
    )
}

fn parse_i64(value: Option<&String>) -> i64 {
    value
        .and_then(|value| value.parse().ok())
        .unwrap_or_default()
}

fn member_sub_type_label(value: &str) -> &str {
    match value {
        "approve" => "管理员同意",
        "invite" => "成员邀请",
        "leave" => "主动退群",
        "kick" => "被移出群",
        "kick_me" => "机器人被移出",
        _ => "群事件",
    }
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
    fn renders_join_member_card_svg() {
        let card = MemberCard {
            kind: MemberCardKind::Join,
            group_id: 100,
            user_id: 42,
            operator_id: Some(7),
            sub_type: Some("approve".to_string()),
            nickname: Some("Alice".to_string()),
            card: Some("小爱".to_string()),
            level: Some("66".to_string()),
            title: Some("群星".to_string()),
            avatar_data_uri: None,
        };

        let svg = render_member_card_svg(&card);

        assert!(svg.contains("欢迎新成员入群"));
        assert!(svg.contains("QQ 号：42"));
        assert!(svg.contains("小爱"));
        assert!(svg.contains("等级：66"));
        assert!(svg.contains("管理员同意"));
    }

    #[test]
    fn renders_member_card_png() {
        let card = MemberCard {
            kind: MemberCardKind::Leave,
            group_id: 100,
            user_id: 42,
            operator_id: None,
            sub_type: Some("leave".to_string()),
            nickname: None,
            card: None,
            level: None,
            title: None,
            avatar_data_uri: None,
        };

        let png = render_member_card_png(&card).unwrap();

        assert!(png.starts_with(b"\x89PNG"));
    }

    #[test]
    fn round_trips_member_card_query() {
        let card = MemberCard {
            kind: MemberCardKind::Leave,
            group_id: 100,
            user_id: 42,
            operator_id: None,
            sub_type: Some("leave".to_string()),
            nickname: Some("Alice".to_string()),
            card: None,
            level: Some("9".to_string()),
            title: None,
            avatar_data_uri: None,
        };

        assert_eq!(member_card_from_query(&member_card_query(&card)), card);
    }
}
