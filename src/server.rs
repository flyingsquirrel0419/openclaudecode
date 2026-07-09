use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;
use subtle::ConstantTimeEq;

use crate::{
    config::{Config, RuntimeState, remove_runtime, write_runtime},
    models::{MessageRequest, estimate_tokens},
    providers::ProviderRuntime,
};

#[derive(Clone)]
struct AppState {
    config: Config,
    providers: ProviderRuntime,
}

pub async fn serve(addr: SocketAddr, config: Config) -> anyhow::Result<()> {
    write_runtime(&RuntimeState {
        pid: std::process::id(),
        host: config.host.clone(),
        port: config.port,
    })?;
    let state = AppState {
        providers: ProviderRuntime::new(config.clone())?,
        config,
    };
    let app = Router::new()
        .route("/", get(root))
        .route("/healthz", get(healthz))
        .route("/v1/models", get(models))
        .route("/v1/messages", post(messages))
        .route("/v1/messages/count_tokens", post(count_tokens))
        .route("/api/config", get(api_config))
        .route("/api/providers", get(api_providers))
        .route("/api/stop", post(api_stop))
        .with_state(Arc::new(state));

    tracing::info!("openclaude listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("bind listener")?;
    let result = axum::serve(listener, app).await.context("serve");
    let _ = remove_runtime();
    result
}

async fn root() -> impl IntoResponse {
    Json(json!({
        "service": "openclaude",
        "message": "Claude Code gateway is running"
    }))
}

async fn healthz(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(json!({
        "service": "openclaude",
        "status": "ok",
        "port": state.config.port,
        "providers": state.config.providers.len()
    }))
}

async fn models(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Err(resp) = authorize(&state.config, &headers) {
        return *resp;
    }
    let models = state
        .providers
        .list_models()
        .into_iter()
        .map(|id| {
            json!({
                "type": "model",
                "id": id,
                "display_name": id,
                "created_at": "2026-01-01T00:00:00Z"
            })
        })
        .collect::<Vec<_>>();
    Json(json!({
        "data": models,
        "has_more": false
    }))
    .into_response()
}

async fn count_tokens(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<MessageRequest>,
) -> Response {
    if let Err(resp) = authorize(&state.config, &headers) {
        return *resp;
    }
    Json(json!({ "input_tokens": estimate_tokens(&req) })).into_response()
}

async fn messages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<MessageRequest>,
) -> Response {
    if let Err(resp) = authorize(&state.config, &headers) {
        return *resp;
    }
    match state.providers.execute(req, &headers).await {
        Ok(resp) => resp,
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "type": "error",
                "error": {
                    "type": "api_error",
                    "message": err.to_string()
                }
            })),
        )
            .into_response(),
    }
}

async fn api_config(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Err(resp) = authorize(&state.config, &headers) {
        return *resp;
    }
    Json(json!({
        "host": state.config.host,
        "port": state.config.port,
        "defaultProvider": state.config.default_provider,
        "providerCount": state.config.providers.len()
    }))
    .into_response()
}

async fn api_providers(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Err(resp) = authorize(&state.config, &headers) {
        return *resp;
    }
    Json(json!({ "providers": state.config.providers })).into_response()
}

async fn api_stop(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Err(resp) = authorize(&state.config, &headers) {
        return *resp;
    }
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let _ = remove_runtime();
        std::process::exit(0);
    });
    Json(json!({ "ok": true })).into_response()
}

fn authorize(config: &Config, headers: &HeaderMap) -> Result<(), Box<Response>> {
    let expected = &config.gateway_token;
    let got = headers
        .get("x-api-key")
        .or_else(|| headers.get("authorization"))
        .and_then(|value| value.to_str().ok())
        .map(normalize_auth_value);
    if got
        .map(|value| constant_time_token_eq(expected, value))
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err(Box::new(
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "type": "error",
                    "error": {
                        "type": "authentication_error",
                        "message": "invalid openclaude gateway token"
                    }
                })),
            )
                .into_response(),
        ))
    }
}

fn normalize_auth_value(value: &str) -> &str {
    value
        .trim()
        .strip_prefix("Bearer ")
        .unwrap_or_else(|| value.trim())
        .trim()
}

fn constant_time_token_eq(expected: &str, got: &str) -> bool {
    expected.len() == got.len() && expected.as_bytes().ct_eq(got.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::{AUTHORIZATION, HeaderValue};

    fn config_with_token(token: &str) -> Config {
        let mut cfg = Config::default_config();
        cfg.gateway_token = token.to_string();
        cfg
    }

    #[test]
    fn authorization_accepts_x_api_key() {
        let cfg = config_with_token("occ_test_token");
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_static("occ_test_token"));
        assert!(authorize(&cfg, &headers).is_ok());
    }

    #[test]
    fn authorization_accepts_bearer_token() {
        let cfg = config_with_token("occ_test_token");
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer occ_test_token"),
        );
        assert!(authorize(&cfg, &headers).is_ok());
    }

    #[test]
    fn authorization_rejects_wrong_token() {
        let cfg = config_with_token("occ_test_token");
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_static("occ_wrong_token"));
        assert!(authorize(&cfg, &headers).is_err());
    }

    #[test]
    fn token_comparison_requires_exact_value() {
        assert!(constant_time_token_eq("occ_abc", "occ_abc"));
        assert!(!constant_time_token_eq("occ_abc", "occ_abd"));
        assert!(!constant_time_token_eq("occ_abc", "occ_abc_extra"));
    }
}
