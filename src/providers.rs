use std::time::Duration;

use anyhow::Context;
use axum::{
    Json,
    body::Body,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Value, json};

use crate::{
    config::{AdapterKind, Config, ProviderConfig},
    models::{MessageRequest, Route, anthropic_sse, extract_text},
};

#[derive(Clone)]
pub struct ProviderRuntime {
    client: reqwest::Client,
    config: Config,
}

impl ProviderRuntime {
    pub fn new(config: Config) -> anyhow::Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(600))
                .build()
                .context("build http client")?,
            config,
        })
    }

    pub fn list_models(&self) -> Vec<String> {
        self.config.claude_model_ids()
    }

    pub fn route(&self, model: &str) -> anyhow::Result<(Route, ProviderConfig)> {
        if let Some((provider_name, model_id)) = model.split_once('/')
            && let Some(provider) = self.config.providers.get(provider_name)
        {
            return Ok((
                Route {
                    provider_name: provider_name.to_string(),
                    model: model_id.to_string(),
                },
                provider.clone(),
            ));
        }
        let provider = self
            .config
            .providers
            .get(&self.config.default_provider)
            .with_context(|| {
                format!("default provider missing: {}", self.config.default_provider)
            })?;
        Ok((
            Route {
                provider_name: self.config.default_provider.clone(),
                model: provider
                    .default_model
                    .clone()
                    .unwrap_or_else(|| model.to_string()),
            },
            provider.clone(),
        ))
    }

    pub async fn execute(
        &self,
        mut req: MessageRequest,
        incoming: &HeaderMap,
    ) -> anyhow::Result<Response> {
        let (route, provider) = self.route(&req.model)?;
        req.model = route.model;
        tracing::info!(provider = %route.provider_name, model = %req.model, "routing Claude Code request");
        match provider.adapter {
            AdapterKind::Anthropic => self.anthropic(provider, req, incoming).await,
            AdapterKind::OpenAiChat | AdapterKind::AzureOpenAi => {
                self.openai_chat(provider, req).await
            }
            AdapterKind::Google => self.google(provider, req).await,
            AdapterKind::Cursor | AdapterKind::Kiro => {
                anyhow::bail!(
                    "adapter {} is planned but not implemented in this Rust milestone",
                    provider.adapter
                )
            }
        }
    }

    async fn anthropic(
        &self,
        provider: ProviderConfig,
        req: MessageRequest,
        incoming: &HeaderMap,
    ) -> anyhow::Result<Response> {
        let url = format!("{}/v1/messages", provider.base_url.trim_end_matches('/'));
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        headers.insert(
            "anthropic-version",
            incoming
                .get("anthropic-version")
                .cloned()
                .unwrap_or_else(|| HeaderValue::from_static("2023-06-01")),
        );
        if let Some(beta) = incoming.get("anthropic-beta") {
            headers.insert("anthropic-beta", beta.clone());
        }
        if let Some(key) = provider.resolve_api_key() {
            headers.insert("x-api-key", HeaderValue::from_str(&key)?);
        }
        let secrets = provider_secrets(&provider);

        let upstream = self
            .client
            .post(url)
            .headers(headers)
            .json(&req)
            .send()
            .await?;
        Ok(proxy_response(upstream, &secrets).await)
    }

    async fn openai_chat(
        &self,
        provider: ProviderConfig,
        req: MessageRequest,
    ) -> anyhow::Result<Response> {
        let url = format!(
            "{}/chat/completions",
            provider.base_url.trim_end_matches('/')
        );
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        if let Some(key) = provider.resolve_api_key() {
            headers.insert(
                "authorization",
                HeaderValue::from_str(&format!("Bearer {key}"))?,
            );
        }
        let secrets = provider_secrets(&provider);

        let stream = req.stream.unwrap_or(false);
        let body = openai_body(&req, stream);
        let upstream = self
            .client
            .post(url)
            .headers(headers)
            .json(&body)
            .send()
            .await?;
        if stream {
            Ok(transform_openai_stream(upstream, &secrets).await)
        } else {
            let status = upstream.status();
            let text = upstream.text().await.unwrap_or_default();
            if !status.is_success() {
                return Ok(upstream_error_response(status, text, &secrets));
            }
            Ok(Json(anthropic_from_openai_json(&req.model, &text)).into_response())
        }
    }

    async fn google(
        &self,
        provider: ProviderConfig,
        req: MessageRequest,
    ) -> anyhow::Result<Response> {
        if req.stream.unwrap_or(false) {
            anyhow::bail!("google streaming is planned but not implemented yet");
        }
        let key = provider
            .resolve_api_key()
            .context("google adapter requires api_key")?;
        let url = format!(
            "{}/v1beta/models/{}:generateContent",
            provider.base_url.trim_end_matches('/'),
            req.model
        );
        let secrets = provider_secrets(&provider);
        let upstream = self
            .client
            .post(url)
            .query(&[("key", key.as_str())])
            .json(&google_body(&req))
            .send()
            .await?;
        let status = upstream.status();
        let text = upstream.text().await.unwrap_or_default();
        if !status.is_success() {
            return Ok(upstream_error_response(status, text, &secrets));
        }
        Ok(Json(anthropic_from_google_json(&req.model, &text)).into_response())
    }
}

fn provider_secrets(provider: &ProviderConfig) -> Vec<String> {
    provider.resolve_api_key().into_iter().collect()
}

fn openai_body(req: &MessageRequest, stream: bool) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = &req.system {
        messages.push(json!({ "role": "system", "content": extract_text(system) }));
    }
    for msg in &req.messages {
        append_openai_messages(&mut messages, msg);
    }

    let mut body = json!({
        "model": req.model,
        "messages": messages,
        "stream": stream,
    });
    if let Some(max_tokens) = req.max_tokens {
        body["max_tokens"] = json!(max_tokens);
    }
    if let Some(tools) = anthropic_tools_to_openai(req.tools.as_ref()) {
        body["tools"] = tools;
    }
    body
}

fn append_openai_messages(messages: &mut Vec<Value>, msg: &crate::models::Message) {
    match msg.role.as_str() {
        "assistant" => {
            let (text, tool_calls) = assistant_text_and_tool_calls(&msg.content);
            let mut out = json!({ "role": "assistant", "content": text });
            if !tool_calls.is_empty() {
                out["tool_calls"] = Value::Array(tool_calls);
            }
            messages.push(out);
        }
        "user" => {
            if let Some(parts) = msg.content.as_array() {
                let mut user_text = Vec::new();
                for part in parts {
                    if part.get("type").and_then(Value::as_str) == Some("tool_result") {
                        let tool_call_id = part
                            .get("tool_use_id")
                            .or_else(|| part.get("tool_call_id"))
                            .and_then(Value::as_str)
                            .unwrap_or("tool_call");
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": tool_call_id,
                            "content": part.get("content").map(extract_text).unwrap_or_default()
                        }));
                    } else if part.get("type").and_then(Value::as_str) == Some("text")
                        && let Some(text) = part.get("text").and_then(Value::as_str)
                    {
                        user_text.push(text.to_string());
                    }
                }
                if !user_text.is_empty() {
                    messages.push(json!({ "role": "user", "content": user_text.join("\n") }));
                }
            } else {
                messages.push(json!({ "role": "user", "content": extract_text(&msg.content) }));
            }
        }
        _ => messages.push(json!({ "role": "user", "content": extract_text(&msg.content) })),
    }
}

fn assistant_text_and_tool_calls(content: &Value) -> (String, Vec<Value>) {
    let Some(parts) = content.as_array() else {
        return (extract_text(content), Vec::new());
    };
    let mut text = Vec::new();
    let mut tool_calls = Vec::new();
    for part in parts {
        match part.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(value) = part.get("text").and_then(Value::as_str) {
                    text.push(value.to_string());
                }
            }
            Some("tool_use") => {
                let id = part
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("tool_call");
                let name = part.get("name").and_then(Value::as_str).unwrap_or("tool");
                let input = part.get("input").cloned().unwrap_or_else(|| json!({}));
                tool_calls.push(json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string())
                    }
                }));
            }
            _ => {}
        }
    }
    (text.join("\n"), tool_calls)
}

fn anthropic_tools_to_openai(tools: Option<&Value>) -> Option<Value> {
    let tools = tools?.as_array()?;
    let mapped = tools
        .iter()
        .filter_map(|tool| {
            let obj = tool.as_object()?;
            let name = obj.get("name")?.as_str()?;
            Some(json!({
                "type": "function",
                "function": {
                    "name": name,
                    "description": obj.get("description").cloned().unwrap_or(Value::String(String::new())),
                    "parameters": obj.get("input_schema").cloned().unwrap_or_else(|| json!({"type":"object"})),
                }
            }))
        })
        .collect::<Vec<_>>();
    (!mapped.is_empty()).then_some(Value::Array(mapped))
}

fn google_body(req: &MessageRequest) -> Value {
    let contents = req
        .messages
        .iter()
        .map(|msg| {
            let role = if msg.role == "assistant" {
                "model"
            } else {
                "user"
            };
            json!({
                "role": role,
                "parts": [{ "text": extract_text(&msg.content) }]
            })
        })
        .collect::<Vec<_>>();
    let mut body = json!({ "contents": contents });
    if let Some(system) = &req.system {
        body["systemInstruction"] = json!({
            "parts": [{ "text": extract_text(system) }]
        });
    }
    if let Some(max_tokens) = req.max_tokens {
        body["generationConfig"] = json!({ "maxOutputTokens": max_tokens });
    }
    body
}

fn anthropic_from_google_json(model: &str, raw: &str) -> Value {
    let parsed: Value = serde_json::from_str(raw).unwrap_or_else(|_| json!({}));
    let text = parsed
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();
    json!({
        "id": format!("msg_{}", uuid::Uuid::new_v4().simple()),
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": [{ "type": "text", "text": text }],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {
            "input_tokens": parsed.pointer("/usageMetadata/promptTokenCount").and_then(Value::as_u64).unwrap_or(0),
            "output_tokens": parsed.pointer("/usageMetadata/candidatesTokenCount").and_then(Value::as_u64).unwrap_or(0)
        }
    })
}

fn anthropic_from_openai_json(model: &str, raw: &str) -> Value {
    let parsed: Value = serde_json::from_str(raw).unwrap_or_else(|_| json!({}));
    let choice = parsed
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let message = choice.get("message").cloned().unwrap_or_else(|| json!({}));
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut content_blocks = Vec::new();
    if !content.is_empty() {
        content_blocks.push(json!({ "type": "text", "text": content }));
    }
    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for call in tool_calls {
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("tool_call");
            let function = call.get("function").cloned().unwrap_or_else(|| json!({}));
            let name = function
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("tool");
            let raw_args = function
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            let input = serde_json::from_str::<Value>(raw_args).unwrap_or_else(|_| json!({}));
            content_blocks.push(json!({
                "type": "tool_use",
                "id": id,
                "name": name,
                "input": input
            }));
        }
    }
    if content_blocks.is_empty() {
        content_blocks.push(json!({ "type": "text", "text": "" }));
    }
    let stop_reason = match choice.get("finish_reason").and_then(Value::as_str) {
        Some("tool_calls") => "tool_use",
        Some("length") => "max_tokens",
        _ => "end_turn",
    };
    json!({
        "id": format!("msg_{}", uuid::Uuid::new_v4().simple()),
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content_blocks,
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": {
            "input_tokens": parsed.pointer("/usage/prompt_tokens").and_then(Value::as_u64).unwrap_or(0),
            "output_tokens": parsed.pointer("/usage/completion_tokens").and_then(Value::as_u64).unwrap_or(0)
        }
    })
}

async fn proxy_response(upstream: reqwest::Response, secrets: &[String]) -> Response {
    let status = upstream.status();
    if !status.is_success() {
        let text = upstream.text().await.unwrap_or_default();
        return upstream_error_response(status, text, secrets);
    }
    let mut builder = Response::builder().status(status);
    for (name, value) in upstream.headers() {
        if name.as_str().eq_ignore_ascii_case("content-length") {
            continue;
        }
        if let Ok(header_name) = HeaderName::from_bytes(name.as_str().as_bytes()) {
            builder = builder.header(header_name, value.clone());
        }
    }
    let stream = upstream
        .bytes_stream()
        .map(|chunk| chunk.map_err(std::io::Error::other));
    builder.body(Body::from_stream(stream)).unwrap()
}

async fn transform_openai_stream(upstream: reqwest::Response, secrets: &[String]) -> Response {
    let status = upstream.status();
    if !status.is_success() {
        let text = upstream.text().await.unwrap_or_default();
        return upstream_error_response(status, text, secrets);
    }

    let stream = async_stream::stream! {
        let id = format!("msg_{}", uuid::Uuid::new_v4().simple());
        yield Ok::<Bytes, std::io::Error>(Bytes::from(anthropic_sse("message_start", json!({
            "type": "message_start",
            "message": {
                "id": id,
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": "routed",
                "stop_reason": null,
                "stop_sequence": null,
                "usage": { "input_tokens": 0, "output_tokens": 0 }
            }
        }))));
        yield Ok(Bytes::from(anthropic_sse("content_block_start", json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": { "type": "text", "text": "" }
        }))));

        let mut buf = String::new();
        let mut chunks = upstream.bytes_stream();
        while let Some(chunk) = chunks.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(err) => {
                    yield Ok(Bytes::from(anthropic_sse("error", json!({
                        "type": "error",
                        "error": { "type": "api_error", "message": err.to_string() }
                    }))));
                    break;
                }
            };
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(pos) = buf.find('\n') {
                let line = buf[..pos].trim().to_string();
                buf = buf[pos + 1..].to_string();
                if !line.starts_with("data:") {
                    continue;
                }
                let data = line.trim_start_matches("data:").trim();
                if data == "[DONE]" {
                    break;
                }
                let Ok(value) = serde_json::from_str::<Value>(data) else { continue };
                if let Some(text) = value.pointer("/choices/0/delta/content").and_then(Value::as_str) {
                    yield Ok(Bytes::from(anthropic_sse("content_block_delta", json!({
                        "type": "content_block_delta",
                        "index": 0,
                        "delta": { "type": "text_delta", "text": text }
                    }))));
                }
            }
        }

        yield Ok(Bytes::from(anthropic_sse("content_block_stop", json!({
            "type": "content_block_stop",
            "index": 0
        }))));
        yield Ok(Bytes::from(anthropic_sse("message_delta", json!({
            "type": "message_delta",
            "delta": { "stop_reason": "end_turn", "stop_sequence": null },
            "usage": { "output_tokens": 0 }
        }))));
        yield Ok(Bytes::from(anthropic_sse("message_stop", json!({
            "type": "message_stop"
        }))));
    };

    Response::builder()
        .status(200)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .body(Body::from_stream(stream))
        .unwrap()
}

fn upstream_error_response(
    status: reqwest::StatusCode,
    text: String,
    secrets: &[String],
) -> Response {
    (status, redact_sensitive_text(&text, secrets)).into_response()
}

fn redact_sensitive_text(text: &str, secrets: &[String]) -> String {
    let mut redacted = text.to_string();
    for secret in secrets {
        if !secret.is_empty() {
            redacted = redacted.replace(secret, "[redacted]");
        }
    }
    redacted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_basic_openai_body() {
        let req = MessageRequest {
            model: "gpt-test".to_string(),
            messages: vec![crate::models::Message {
                role: "user".to_string(),
                content: json!("hello"),
            }],
            system: Some(json!("system")),
            max_tokens: Some(42),
            stream: Some(false),
            tools: None,
            tool_choice: None,
            extra: Default::default(),
        };
        let body = openai_body(&req, false);
        assert_eq!(body["model"], "gpt-test");
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert_eq!(body["max_tokens"], 42);
    }

    #[test]
    fn maps_google_body_and_response() {
        let req = MessageRequest {
            model: "gemini-test".to_string(),
            messages: vec![crate::models::Message {
                role: "user".to_string(),
                content: json!([{ "type": "text", "text": "hello" }]),
            }],
            system: Some(json!("system")),
            max_tokens: Some(12),
            stream: Some(false),
            tools: None,
            tool_choice: None,
            extra: Default::default(),
        };
        let body = google_body(&req);
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(body["contents"][0]["parts"][0]["text"], "hello");
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "system");
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 12);

        let out = anthropic_from_google_json(
            "gemini-test",
            r#"{"candidates":[{"content":{"parts":[{"text":"hi"}]}}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":2}}"#,
        );
        assert_eq!(out["content"][0]["text"], "hi");
        assert_eq!(out["usage"]["input_tokens"], 3);
        assert_eq!(out["usage"]["output_tokens"], 2);
    }

    #[test]
    fn maps_openai_response_to_anthropic_shape() {
        let out = anthropic_from_openai_json(
            "gpt-test",
            r#"{"choices":[{"message":{"content":"hello"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":7}}"#,
        );
        assert_eq!(out["type"], "message");
        assert_eq!(out["content"][0]["text"], "hello");
        assert_eq!(out["stop_reason"], "end_turn");
        assert_eq!(out["usage"]["input_tokens"], 5);
        assert_eq!(out["usage"]["output_tokens"], 7);
    }

    #[test]
    fn maps_openai_tool_calls_and_results() {
        let req = MessageRequest {
            model: "gpt-test".to_string(),
            messages: vec![
                crate::models::Message {
                    role: "assistant".to_string(),
                    content: json!([
                        { "type": "text", "text": "checking" },
                        { "type": "tool_use", "id": "call_1", "name": "read_file", "input": { "path": "a.txt" } }
                    ]),
                },
                crate::models::Message {
                    role: "user".to_string(),
                    content: json!([
                        { "type": "tool_result", "tool_use_id": "call_1", "content": "ok" }
                    ]),
                },
            ],
            system: None,
            max_tokens: None,
            stream: Some(false),
            tools: None,
            tool_choice: None,
            extra: Default::default(),
        };
        let body = openai_body(&req, false);
        assert_eq!(body["messages"][0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(
            body["messages"][0]["tool_calls"][0]["function"]["name"],
            "read_file"
        );
        assert_eq!(body["messages"][1]["role"], "tool");
        assert_eq!(body["messages"][1]["tool_call_id"], "call_1");

        let out = anthropic_from_openai_json(
            "gpt-test",
            r#"{"choices":[{"message":{"tool_calls":[{"id":"call_2","type":"function","function":{"name":"write_file","arguments":"{\"path\":\"b.txt\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":1,"completion_tokens":2}}"#,
        );
        assert_eq!(out["stop_reason"], "tool_use");
        assert_eq!(out["content"][0]["type"], "tool_use");
        assert_eq!(out["content"][0]["name"], "write_file");
        assert_eq!(out["content"][0]["input"]["path"], "b.txt");
    }

    #[test]
    fn redacts_provider_secrets_from_upstream_errors() {
        let text = "request failed for https://example.test?key=super-secret-key";
        let out = redact_sensitive_text(text, &["super-secret-key".to_string()]);
        assert_eq!(
            out,
            "request failed for https://example.test?key=[redacted]"
        );
    }
}
