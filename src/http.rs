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
    config::{GithubConfig, NotifyTarget},
    github,
    notifier::Notifier,
};

#[derive(Clone)]
pub struct AppState {
    github: Arc<GithubConfig>,
    notifier: Arc<Notifier>,
    public_base_url: Arc<str>,
}

impl AppState {
    pub fn new(
        github: Arc<GithubConfig>,
        notifier: Arc<Notifier>,
        public_base_url: String,
    ) -> Self {
        Self {
            github,
            notifier,
            public_base_url: public_base_url.into(),
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
        .route("/github/change.svg", get(github_change_svg))
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
    message: Option<String>,
    user_id: Option<i64>,
    self_id: Option<i64>,
    group_id: Option<i64>,
}

async fn qq_event(State(state): State<AppState>, Json(event): Json<QqEvent>) -> impl IntoResponse {
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

#[derive(Debug, Deserialize)]
struct CardQuery {
    url: String,
}

async fn github_card(Query(query): Query<CardQuery>) -> impl IntoResponse {
    match github::fetch_repo_card(&query.url).await {
        Ok(card) => Html(github::render_repo_card_html(&card)).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": error.to_string() })),
        )
            .into_response(),
    }
}

async fn github_card_svg(Query(query): Query<CardQuery>) -> impl IntoResponse {
    match github::fetch_repo_card(&query.url).await {
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
        "[CQ:image,file={}/github/card.svg?url={}]",
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
        bot::MockBot,
        config::{FeatureConfig, RepositoryConfig, SimpleRepositoryConfig},
    };
    use axum::{
        body::Body,
        http::{Request, header::CONTENT_TYPE},
    };
    use tower::ServiceExt;

    fn test_state() -> AppState {
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
            Arc::new(Notifier::new(Arc::new(MockBot::default()))),
            "http://127.0.0.1:8080".to_string(),
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
            message: Some("[CQ:at,qq=123] ping".to_string()),
            user_id: Some(42),
            self_id: Some(123),
            group_id: Some(100),
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
            message: Some("[CQ:at,qq=999] ping".to_string()),
            user_id: Some(42),
            self_id: Some(123),
            group_id: Some(100),
        };

        assert_eq!(qq_reply(&config, &event, "http://127.0.0.1:8080"), None);
    }
}
