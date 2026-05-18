//! OpenRouter / OpenAI-compatible LLM provider with streaming.

use std::pin::Pin;
use std::sync::Arc;

use agtrs::prelude::*;
use async_stream::stream;
use futures::Stream;
use injectable::prelude::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::AppConfig;

injectable::bind!(dyn LlmProvider => OpenRouterProvider);

#[derive(Clone)]
pub struct OpenRouterProvider {
    config: Arc<AppConfig>,
    client: Client,
}

#[injectable]
impl OpenRouterProvider {
    #[injectable(ctor)]
    pub fn new(#[injectable(inject)] config: Arc<AppConfig>) -> Self {
        Self { config, client: Client::new() }
    }
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Value>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<UsageInfo>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChatMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChatMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct UsageInfo {
    prompt_tokens: usize,
    completion_tokens: usize,
}

#[derive(Deserialize)]
struct StreamChunkData {
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<UsageInfo>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
}

fn message_to_json(msg: &Message) -> Value {
    let role = match msg.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    };
    let text = match &msg.content {
        MessageContent::Text(t) => t.clone(),
        MessageContent::MultiPart(blocks) => blocks.iter().filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.clone()),
            ContentBlock::ToolResult { content, .. } => Some(content.clone()),
            _ => None,
        }).collect::<Vec<_>>().join(""),
    };
    let mut obj = json!({ "role": role, "content": text });
    if let Some(id) = &msg.tool_call_id {
        obj["tool_call_id"] = json!(id);
    }
    obj
}

fn finish_to_stop(s: Option<&str>) -> StopReason {
    match s {
        Some("tool_calls") | Some("tool_use") => StopReason::ToolUse,
        Some("length") | Some("max_tokens") => StopReason::MaxTokens,
        _ => StopReason::EndTurn,
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenRouterProvider {
    async fn complete(
        &self,
        messages: &[Message],
        options: &LlmOptions,
    ) -> Result<LlmResponse, AgtrsError> {
        let body = ChatRequest {
            model: self.config.llm_model.clone(),
            messages: messages.iter().map(message_to_json).collect(),
            temperature: options.temperature,
            max_tokens: options.max_tokens,
            stream: false,
        };

        let resp = self.client
            .post(format!("{}/chat/completions", self.config.llm_base_url))
            .header("Authorization", format!("Bearer {}", self.config.openrouter_api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| AgtrsError::LlmCallFailed { reason: format!("HTTP: {e}") })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AgtrsError::LlmCallFailed { reason: format!("{status}: {text}") });
        }

        let data: ChatResponse = resp.json().await
            .map_err(|e| AgtrsError::LlmCallFailed { reason: format!("Parse: {e}") })?;

        let choice = data.choices.into_iter().next()
            .ok_or_else(|| AgtrsError::LlmCallFailed { reason: "No choices".into() })?;

        let text = choice.message.content.unwrap_or_default();
        let finish_reason = finish_to_stop(choice.finish_reason.as_deref());
        let usage = data.usage.map(|u| TokenUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens, cache_read_tokens: 0, cache_write_tokens: 0,
        }).unwrap_or_default();

        Ok(LlmResponse {
            message: Message {
                role: Role::Assistant,
                content: MessageContent::Text(text),
                name: None,
                tool_call_id: None,
                metadata: Default::default(),
            },
            usage,
            tool_calls: vec![],
            finish_reason,
            thinking_blocks: vec![],
        })
    }

    async fn stream(
        &self,
        messages: &[Message],
        options: &LlmOptions,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, AgtrsError>> + Send>>, AgtrsError> {
        let body = ChatRequest {
            model: self.config.llm_model.clone(),
            messages: messages.iter().map(message_to_json).collect(),
            temperature: options.temperature,
            max_tokens: options.max_tokens,
            stream: true,
        };

        let resp = self.client
            .post(format!("{}/chat/completions", self.config.llm_base_url))
            .header("Authorization", format!("Bearer {}", self.config.openrouter_api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| AgtrsError::LlmCallFailed { reason: format!("HTTP stream: {e}") })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AgtrsError::LlmCallFailed { reason: format!("Stream {status}: {text}") });
        }

        let mut byte_stream = resp.bytes_stream();
        let s = stream! {
            use futures::StreamExt;
            let mut buf = String::new();
            while let Some(chunk) = byte_stream.next().await {
                let bytes = match chunk {
                    Ok(b) => b,
                    Err(e) => {
                        yield Err(AgtrsError::LlmCallFailed { reason: format!("Stream read: {e}") });
                        return;
                    }
                };
                buf.push_str(&String::from_utf8_lossy(&bytes));
                while let Some(pos) = buf.find("\n\n") {
                    let event = buf[..pos].trim().to_string();
                    buf = buf[pos + 2..].to_string();
                    for line in event.lines() {
                        let data = match line.strip_prefix("data: ") {
                            Some(d) => d.trim(),
                            None => continue,
                        };
                        if data == "[DONE]" { return; }
                        let cd: StreamChunkData = match serde_json::from_str(data) {
                            Ok(d) => d,
                            Err(_) => continue,
                        };
                        let usage = cd.usage.map(|u| TokenUsage {
                            input_tokens: u.prompt_tokens,
                            output_tokens: u.completion_tokens, cache_read_tokens: 0, cache_write_tokens: 0,
                        });
                        let choice = match cd.choices.into_iter().next() {
                            Some(c) => c,
                            None => continue,
                        };
                        let stop_reason = Some(finish_to_stop(choice.finish_reason.as_deref()));
                        yield Ok(StreamChunk {
                            delta: choice.delta.content.unwrap_or_default(),
                            thinking_delta: None,
                            tool_call_delta: None,
                            stop_reason,
                            usage,
                        });
                    }
                }
            }
        };
        Ok(Box::pin(s))
    }

    async fn embed(&self, _inputs: &[String], _model: Option<&str>) -> Result<Vec<Embedding>, AgtrsError> {
        Err(AgtrsError::msg("Embeddings not supported"))
    }

    async fn count_tokens(&self, _messages: &[Message]) -> Result<usize, AgtrsError> {
        Err(AgtrsError::msg("Token counting not supported"))
    }

    fn context_window(&self) -> usize { 128_000 }
    fn model(&self) -> &str { &self.config.llm_model }
}
