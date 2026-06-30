use std::{net::SocketAddr, sync::Arc};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::json;
use sha2::Sha256;

use crate::{
    config::{GithubConfig, ModerationConfig, NotifyTarget},
    github,
    notifier::Notifier,
    qq::{self, MemberCard, MemberCardKind},
    scheduler::ScheduleRuntime,
};

#[derive(Clone)]
pub struct AppState {
    github: Arc<GithubConfig>,
    notifier: Arc<Notifier>,
    github_client: reqwest::Client,
    schedule: Option<Arc<ScheduleRuntime>>,
    public_base_url: Arc<str>,
    moderation: Arc<ModerationConfig>,
}

impl AppState {
    pub fn new(
        github: Arc<GithubConfig>,
        notifier: Arc<Notifier>,
        github_client: reqwest::Client,
        schedule: Option<Arc<ScheduleRuntime>>,
        public_base_url: String,
        moderation: ModerationConfig,
    ) -> Self {
        Self {
            github,
            notifier,
            github_client,
            schedule,
            public_base_url: public_base_url.into(),
            moderation: Arc::new(moderation),
        }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/github/webhook", post(github_webhook))
        .route("/qq/event", post(qq_event))
        .route("/github/card", get(github_card))
        .route("/github/card.svg", get(github_card_svg))
        .route("/github/card.png", get(github_card_png))
        .route("/github/change.svg", get(github_change_svg))
        .route("/github/change.png", get(github_change_png))
        .route("/qq/member.png", get(qq_member_png))
        .with_state(state)
}

pub async fn serve(address: SocketAddr, state: AppState) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

async fn github_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if let Err(error) = verify_signature(&state.github.webhook_secret, &headers, &body) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": error.to_string() })),
        )
            .into_response();
    }

    let event = headers
        .get("x-github-event")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    match github::parse_event(event, &body) {
        Ok(Some(notification)) => {
            match state.notifier.dispatch(&state.github, notification).await {
                Ok(sent) => (StatusCode::OK, Json(json!({ "sent": sent }))).into_response(),
                Err(error) => (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({ "error": error.to_string() })),
                )
                    .into_response(),
            }
        }
        Ok(None) => (StatusCode::ACCEPTED, Json(json!({ "ignored": event }))).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": error.to_string() })),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct QqEvent {
    post_type: Option<String>,
    notice_type: Option<String>,
    sub_type: Option<String>,
    message: Option<String>,
    raw_message: Option<String>,
    message_id: Option<i64>,
    user_id: Option<i64>,
    self_id: Option<i64>,
    group_id: Option<i64>,
    operator_id: Option<i64>,
}

async fn qq_event(State(state): State<AppState>, Json(event): Json<QqEvent>) -> impl IntoResponse {
    if event.post_type.as_deref() == Some("message")
        && let Some(response) = moderate_qq_message(&state, &event).await
    {
        return response;
    }

    if event.post_type.as_deref() == Some("message")
        && let (Some(schedule), Some(group_id)) = (&state.schedule, event.group_id)
        && let Err(error) = schedule.maybe_send_late_reminder(group_id).await
    {
        tracing::warn!(group_id, ?error, "failed to send late-night reminder");
    }

    if let Some(message) = qq_member_notice(&state, &event).await {
        let Some(group_id) = event.group_id else {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "missing group_id" })),
            )
                .into_response();
        };
        match state
            .notifier
            .send_direct(&NotifyTarget::Group { id: group_id }, &message)
            .await
        {
            Ok(()) => return (StatusCode::OK, Json(json!({ "sent": true }))).into_response(),
            Err(error) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({ "error": error.to_string() })),
                )
                    .into_response();
            }
        }
    }

    let Some(reply) = qq_reply(&state.github, &event, &state.public_base_url) else {
        return (StatusCode::ACCEPTED, Json(json!({ "ignored": true }))).into_response();
    };

    let target = if let Some(group_id) = event.group_id {
        NotifyTarget::Group { id: group_id }
    } else if let Some(user_id) = event.user_id {
        NotifyTarget::Private { id: user_id }
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "missing target" })),
        )
            .into_response();
    };

    match state.notifier.send_direct(&target, &reply).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "sent": true }))).into_response(),
        Err(error) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": error.to_string() })),
        )
            .into_response(),
    }
}

async fn moderate_qq_message(
    state: &AppState,
    event: &QqEvent,
) -> Option<axum::response::Response> {
    let group_id = event.group_id?;
    if !configured_group_ids(&state.github).contains(&group_id) {
        return None;
    }
    let message = event.raw_message.as_deref().or(event.message.as_deref())?;
    let matched_word = state.moderation.matched_word(message)?;
    let Some(message_id) = event.message_id else {
        tracing::warn!(
            group_id,
            user_id = event.user_id,
            matched_word,
            "forbidden-word message has no message_id to recall"
        );
        return Some(
            (
                StatusCode::ACCEPTED,
                Json(json!({ "missing_message_id": true })),
            )
                .into_response(),
        );
    };

    if state.moderation.recall
        && let Err(error) = state.notifier.delete_message(message_id).await
    {
        return Some(
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": error.to_string() })),
            )
                .into_response(),
        );
    }

    if state.moderation.warn {
        let user_id = event.user_id.unwrap_or_default();
        let warning_text = if state.moderation.recall {
            "请注意群内发言，消息包含违禁词已撤回。"
        } else {
            "请注意群内发言，消息包含违禁词。"
        };
        let warning = format!("[CQ:at,qq={user_id}] {warning_text}");
        if let Err(error) = state
            .notifier
            .send_direct(&NotifyTarget::Group { id: group_id }, &warning)
            .await
        {
            return Some(
                (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({ "error": error.to_string() })),
                )
                    .into_response(),
            );
        }
    }

    tracing::info!(
        group_id,
        message_id,
        user_id = event.user_id,
        matched_word,
        "recalled forbidden-word message"
    );
    Some((StatusCode::OK, Json(json!({ "recalled": true }))).into_response())
}

#[derive(Debug, Deserialize)]
struct CardQuery {
    url: String,
}

async fn github_card(
    State(state): State<AppState>,
    Query(query): Query<CardQuery>,
) -> impl IntoResponse {
    match github::fetch_repo_card(&state.github_client, &query.url).await {
        Ok(card) => Html(github::render_repo_card_html(&card)).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": error.to_string() })),
        )
            .into_response(),
    }
}

async fn github_card_svg(
    State(state): State<AppState>,
    Query(query): Query<CardQuery>,
) -> impl IntoResponse {
    match github::fetch_repo_card(&state.github_client, &query.url).await {
        Ok(card) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "image/svg+xml; charset=utf-8")],
            github::render_repo_card_svg(&card),
        )
            .into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": error.to_string() })),
        )
            .into_response(),
    }
}

async fn github_card_png(
    State(state): State<AppState>,
    Query(query): Query<CardQuery>,
) -> impl IntoResponse {
    match github::fetch_repo_card(&state.github_client, &query.url).await {
        Ok(card) => match github::render_repo_card_png(&card) {
            Ok(png) => (StatusCode::OK, [(header::CONTENT_TYPE, "image/png")], png).into_response(),
            Err(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": error.to_string() })),
            )
                .into_response(),
        },
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": error.to_string() })),
        )
            .into_response(),
    }
}

async fn github_change_svg(
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let encoded = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(query.iter())
        .finish();
    let card = github::change_card_from_query(&encoded);
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/svg+xml; charset=utf-8")],
        github::render_change_card_svg(&card),
    )
        .into_response()
}

async fn github_change_png(
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let encoded = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(query.iter())
        .finish();
    let card = github::change_card_from_query(&encoded);
    match github::render_change_card_png(&card) {
        Ok(png) => (StatusCode::OK, [(header::CONTENT_TYPE, "image/png")], png).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error.to_string() })),
        )
            .into_response(),
    }
}

async fn qq_member_png(
    State(state): State<AppState>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let encoded = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(query.iter())
        .finish();
    let mut card = qq::member_card_from_query(&encoded);
    if let Err(error) = qq::hydrate_member_card_avatar(&state.github_client, &mut card).await {
        tracing::warn!(
            user_id = card.user_id,
            ?error,
            "failed to hydrate QQ member avatar"
        );
    }
    match qq::render_member_card_png(&card) {
        Ok(png) => (StatusCode::OK, [(header::CONTENT_TYPE, "image/png")], png).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error.to_string() })),
        )
            .into_response(),
    }
}

async fn qq_member_notice(state: &AppState, event: &QqEvent) -> Option<String> {
    if event.post_type.as_deref() != Some("notice") {
        return None;
    }
    let group_id = event.group_id?;
    if !configured_group_ids(&state.github).contains(&group_id) {
        return None;
    }

    let (kind, text) = match event.notice_type.as_deref()? {
        "group_increase" => (
            MemberCardKind::Join,
            format!("欢迎 [CQ:at,qq={}] 加入本群。", event.user_id?),
        ),
        "group_decrease" => (
            MemberCardKind::Leave,
            format!("成员 {} 已离开本群。", event.user_id?),
        ),
        _ => return None,
    };
    let card = MemberCard {
        kind,
        group_id,
        user_id: event.user_id?,
        operator_id: event.operator_id,
        sub_type: event.sub_type.clone(),
        nickname: None,
        card: None,
        level: None,
        title: None,
        avatar_data_uri: None,
    };
    let card = match state
        .notifier
        .group_member_profile(group_id, card.user_id)
        .await
    {
        Ok(Some(profile)) => MemberCard {
            nickname: profile.nickname,
            card: profile.card,
            level: profile.level,
            title: profile.title,
            ..card
        },
        Ok(None) => card,
        Err(error) => {
            tracing::warn!(
                group_id,
                user_id = card.user_id,
                ?error,
                "failed to fetch QQ member profile"
            );
            card
        }
    };

    Some(format!(
        "{}\n[CQ:image,file={}/qq/member.png?{}]",
        text,
        state.public_base_url.trim_end_matches('/'),
        qq::member_card_query(&card)
    ))
}

fn configured_group_ids(config: &GithubConfig) -> std::collections::HashSet<i64> {
    config
        .repositories
        .iter()
        .flat_map(|repository| repository.targets.iter())
        .filter_map(|target| match target {
            NotifyTarget::Group { id } => Some(*id),
            NotifyTarget::Private { .. } => None,
        })
        .collect()
}

fn qq_reply(config: &GithubConfig, event: &QqEvent, public_base_url: &str) -> Option<String> {
    if event.post_type.as_deref() == Some("notice") && event.notice_type.as_deref() == Some("poke")
    {
        return Some("戳我干嘛".to_string());
    }

    let message = event.message.as_deref()?;
    if let Some(self_id) = event.self_id
        && message_mentions_qq(message, self_id)
    {
        return Some(format!("[CQ:at,qq={}]", event.user_id?));
    }

    if let Some(command) = message.strip_prefix("/repo-guardian ") {
        if !config.admins.contains(&event.user_id?) {
            return Some("没有权限执行管理员指令".to_string());
        }
        return Some(match command.trim() {
            "ping" => "pong".to_string(),
            "repos" => config
                .repositories
                .iter()
                .map(|repo| repo.full_name.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            _ => "未知指令，可用: ping, repos".to_string(),
        });
    }

    find_github_url(message).map(|url| render_qq_repo_card_message(public_base_url, url))
}

fn render_qq_repo_card_message(public_base_url: &str, url: &str) -> String {
    let encoded_url = url::form_urlencoded::byte_serialize(url.as_bytes()).collect::<String>();
    format!(
        "[CQ:image,file={}/github/card.png?url={}]",
        public_base_url.trim_end_matches('/'),
        encoded_url
    )
}

fn message_mentions_qq(message: &str, qq: i64) -> bool {
    let target = qq.to_string();
    message.split("[CQ:at,").skip(1).any(|segment| {
        segment
            .split_once(']')
            .map(|(params, _)| {
                params
                    .split(',')
                    .any(|param| param.trim() == format!("qq={target}"))
            })
            .unwrap_or(false)
    })
}

fn find_github_url(message: &str) -> Option<&str> {
    message.split_whitespace().find(|part| {
        part.starts_with("https://github.com/") || part.starts_with("http://github.com/")
    })
}

fn verify_signature(
    secret: &Option<String>,
    headers: &HeaderMap,
    body: &[u8],
) -> anyhow::Result<()> {
    let Some(secret) = secret else {
        return Ok(());
    };
    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("sha256="))
        .ok_or_else(|| anyhow::anyhow!("missing x-hub-signature-256"))?;

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())?;
    mac.update(body);
    let expected = format!("{:x}", mac.finalize().into_bytes());
    if expected != signature {
        anyhow::bail!("signature mismatch");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bot::{GroupMemberProfile, MockBot},
        config::{FeatureConfig, ModerationConfig, RepositoryConfig, SimpleRepositoryConfig},
    };
    use axum::{
        body::Body,
        http::{Request, header::CONTENT_TYPE},
    };
    use tower::ServiceExt;

    fn test_state() -> AppState {
        test_state_with_bot_and_moderation(
            Arc::new(MockBot::default()),
            ModerationConfig::default(),
        )
    }

    fn test_state_with_bot_and_moderation(
        bot: Arc<MockBot>,
        moderation: ModerationConfig,
    ) -> AppState {
        let github = GithubConfig {
            webhook_secret: None,
            default_features: FeatureConfig::default(),
            admins: [42].into_iter().collect(),
            repositories: vec![RepositoryConfig::from(SimpleRepositoryConfig {
                github: "octo".to_string(),
                repo: "repo".to_string(),
                groups: vec![100],
                privates: vec![],
            })],
        };
        AppState::new(
            Arc::new(github),
            Arc::new(Notifier::new(bot)),
            reqwest::Client::new(),
            None,
            "http://127.0.0.1:8080".to_string(),
            moderation,
        )
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn webhook_accepts_issue_payload() {
        let payload = r#"{"action":"opened","repository":{"full_name":"octo/repo","html_url":"https://github.com/octo/repo"},"issue":{"number":1,"title":"Bug","html_url":"https://github.com/octo/repo/issues/1"},"sender":{"login":"alice"}}"#;
        let response = router(test_state())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/github/webhook")
                    .header("x-github-event", "issues")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(payload))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn replies_only_when_message_mentions_self() {
        let config = test_state().github.as_ref().clone();
        let event = QqEvent {
            post_type: Some("message".to_string()),
            notice_type: None,
            sub_type: None,
            message: Some("[CQ:at,qq=123] ping".to_string()),
            raw_message: Some("[CQ:at,qq=123] ping".to_string()),
            message_id: Some(1),
            user_id: Some(42),
            self_id: Some(123),
            group_id: Some(100),
            operator_id: None,
        };

        assert_eq!(
            qq_reply(&config, &event, "http://127.0.0.1:8080"),
            Some("[CQ:at,qq=42]".to_string())
        );
    }

    #[test]
    fn ignores_messages_that_mention_others() {
        let config = test_state().github.as_ref().clone();
        let event = QqEvent {
            post_type: Some("message".to_string()),
            notice_type: None,
            sub_type: None,
            message: Some("[CQ:at,qq=999] ping".to_string()),
            raw_message: Some("[CQ:at,qq=999] ping".to_string()),
            message_id: Some(1),
            user_id: Some(42),
            self_id: Some(123),
            group_id: Some(100),
            operator_id: None,
        };

        assert_eq!(qq_reply(&config, &event, "http://127.0.0.1:8080"), None);
    }

    #[tokio::test]
    async fn renders_group_join_notice_with_member_card() {
        let bot = Arc::new(MockBot::default());
        bot.set_group_member_profile(
            100,
            42,
            GroupMemberProfile {
                nickname: Some("Alice".to_string()),
                card: Some("小爱".to_string()),
                level: Some("66".to_string()),
                title: Some("群星".to_string()),
            },
        );
        let state = test_state_with_bot_and_moderation(bot, ModerationConfig::default());
        let event = QqEvent {
            post_type: Some("notice".to_string()),
            notice_type: Some("group_increase".to_string()),
            sub_type: Some("approve".to_string()),
            message: None,
            raw_message: None,
            message_id: None,
            user_id: Some(42),
            self_id: Some(123),
            group_id: Some(100),
            operator_id: Some(7),
        };

        let message = qq_member_notice(&state, &event).await.unwrap();

        assert!(message.contains("欢迎 [CQ:at,qq=42] 加入本群。"));
        assert!(message.contains("/qq/member.png?"));
        assert!(message.contains("kind=join"));
        assert!(message.contains("card=%E5%B0%8F%E7%88%B1"));
        assert!(message.contains("level=66"));
    }

    #[tokio::test]
    async fn renders_group_leave_notice_with_member_card() {
        let state = test_state();
        let event = QqEvent {
            post_type: Some("notice".to_string()),
            notice_type: Some("group_decrease".to_string()),
            sub_type: Some("leave".to_string()),
            message: None,
            raw_message: None,
            message_id: None,
            user_id: Some(42),
            self_id: Some(123),
            group_id: Some(100),
            operator_id: None,
        };

        let message = qq_member_notice(&state, &event).await.unwrap();

        assert!(message.contains("成员 42 已离开本群。"));
        assert!(message.contains("/qq/member.png?"));
        assert!(message.contains("kind=leave"));
    }

    #[tokio::test]
    async fn ignores_member_notice_from_unconfigured_group() {
        let state = test_state();
        let event = QqEvent {
            post_type: Some("notice".to_string()),
            notice_type: Some("group_increase".to_string()),
            sub_type: Some("approve".to_string()),
            message: None,
            raw_message: None,
            message_id: None,
            user_id: Some(42),
            self_id: Some(123),
            group_id: Some(999),
            operator_id: Some(7),
        };

        assert_eq!(qq_member_notice(&state, &event).await, None);
    }

    #[tokio::test]
    async fn recalls_forbidden_word_group_message() {
        let bot = Arc::new(MockBot::default());
        let state = test_state_with_bot_and_moderation(
            bot.clone(),
            ModerationConfig {
                enabled: true,
                forbidden_words: vec!["badword".to_string()],
                recall: true,
                warn: true,
            },
        );
        let payload = r#"{"post_type":"message","message_type":"group","group_id":100,"user_id":42,"message_id":88,"message":"hello badword","raw_message":"hello badword"}"#;

        let response = router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/qq/event")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(payload))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(bot.deleted_messages(), vec![88]);
        let messages = bot.messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].1.contains("消息包含违禁词已撤回"));
    }
}
