//! OpenRouter / OpenAI-compatible LLM provider with streaming.

use std::collections::HashMap;
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
        Self {
            config,
            client: Client::new(),
        }
    }
}

// ── Request types ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Value>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
}

// ── Non-streaming response types ───────────────────────────────────────────────

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

#[derive(Deserialize, Default)]
struct ChatMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<OpenAiToolCall>,
}

#[derive(Deserialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "function")]
    function: OpenAiFunction,
}

#[derive(Deserialize)]
struct OpenAiFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct UsageInfo {
    prompt_tokens: usize,
    completion_tokens: usize,
}

// ── Streaming response types ────────────────────────────────────────────────────

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
    #[serde(default)]
    tool_calls: Vec<StreamToolCallDelta>,
}

#[derive(Deserialize, Default)]
struct StreamToolCallDelta {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default, rename = "function")]
    function: StreamToolCallFunction,
}

#[derive(Deserialize, Default)]
struct StreamToolCallFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: String,
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn tool_schema_to_openai(schema: &agtrs_runtime::transport::ToolSchema) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": schema.name,
            "description": schema.description,
            "parameters": schema.input_schema
        }
    })
}

fn message_to_json(msg: &Message) -> Value {
    let role = match msg.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    };

    match &msg.content {
        MessageContent::Text(t) => {
            json!({ "role": role, "content": t })
        }
        MessageContent::MultiPart(blocks) => {
            use agtrs_runtime::transport::ContentBlock;

            // Check if first block is a tool result (role=tool message)
            if let Some(ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            }) = blocks.first()
            {
                return json!({
                    "role": "tool",
                    "tool_call_id": tool_use_id,
                    "content": content
                });
            }

            // Collect text
            let text: String = blocks
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("");

            // Collect tool_use blocks → OpenAI tool_calls format
            let tool_calls: Vec<Value> = blocks
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::ToolUse {
                        tool_use_id,
                        name,
                        input,
                    } => Some(json!({
                        "id": tool_use_id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": serde_json::to_string(input).unwrap_or_default()
                        }
                    })),
                    _ => None,
                })
                .collect();

            if !tool_calls.is_empty() {
                json!({
                    "role": "assistant",
                    "content": if text.is_empty() { Value::Null } else { Value::String(text) },
                    "tool_calls": tool_calls
                })
            } else {
                json!({ "role": role, "content": text })
            }
        }
    }
}

fn finish_to_stop(s: Option<&str>) -> StopReason {
    match s {
        Some("tool_calls") | Some("tool_use") => StopReason::ToolUse,
        Some("length") | Some("max_tokens") => StopReason::MaxTokens,
        _ => StopReason::EndTurn,
    }
}

// ── LlmProvider impl ───────────────────────────────────────────────────────────

#[async_trait::async_trait]
impl LlmProvider for OpenRouterProvider {
    async fn complete(
        &self,
        messages: &[Message],
        options: &LlmOptions,
    ) -> Result<LlmResponse, AgtrsError> {
        let tools: Vec<Value> = options.tools.iter().map(tool_schema_to_openai).collect();
        let tool_choice = if tools.is_empty() {
            None
        } else {
            Some("auto".to_string())
        };

        let body = ChatRequest {
            model: self.config.llm_model.clone(),
            messages: messages.iter().map(message_to_json).collect(),
            temperature: options.temperature,
            max_tokens: options.max_tokens,
            stream: false,
            tools,
            tool_choice,
        };

        let resp = self
            .client
            .post(format!("{}/chat/completions", self.config.llm_base_url))
            .header(
                "Authorization",
                format!("Bearer {}", self.config.openrouter_api_key),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| AgtrsError::LlmCallFailed {
                reason: format!("HTTP: {e}"),
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AgtrsError::LlmCallFailed {
                reason: format!("{status}: {text}"),
            });
        }

        let data: ChatResponse = resp.json().await.map_err(|e| AgtrsError::LlmCallFailed {
            reason: format!("Parse: {e}"),
        })?;

        let choice = data
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AgtrsError::LlmCallFailed {
                reason: "No choices".into(),
            })?;

        let text = choice.message.content.unwrap_or_default();
        let finish_reason = finish_to_stop(choice.finish_reason.as_deref());

        let tool_calls: Vec<ToolCall> = choice
            .message
            .tool_calls
            .into_iter()
            .map(|tc| ToolCall {
                tool_use_id: tc.id,
                name: tc.function.name,
                input: serde_json::from_str(&tc.function.arguments)
                    .unwrap_or(serde_json::Value::Null),
            })
            .collect();

        let usage = data
            .usage
            .map(|u| TokenUsage {
                input_tokens: u.prompt_tokens,
                output_tokens: u.completion_tokens,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
            })
            .unwrap_or_default();

        Ok(LlmResponse {
            message: Message {
                role: Role::Assistant,
                content: MessageContent::Text(text),
                name: None,
                tool_call_id: None,
                metadata: Default::default(),
            },
            usage,
            tool_calls,
            finish_reason,
            thinking_blocks: vec![],
        })
    }

    async fn stream(
        &self,
        messages: &[Message],
        options: &LlmOptions,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, AgtrsError>> + Send>>, AgtrsError>
    {
        let tools: Vec<Value> = options.tools.iter().map(tool_schema_to_openai).collect();
        let tool_choice = if tools.is_empty() {
            None
        } else {
            Some("auto".to_string())
        };

        let body = ChatRequest {
            model: self.config.llm_model.clone(),
            messages: messages.iter().map(message_to_json).collect(),
            temperature: options.temperature,
            max_tokens: options.max_tokens,
            stream: true,
            tools,
            tool_choice,
        };

        let resp = self
            .client
            .post(format!("{}/chat/completions", self.config.llm_base_url))
            .header(
                "Authorization",
                format!("Bearer {}", self.config.openrouter_api_key),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| AgtrsError::LlmCallFailed {
                reason: format!("HTTP stream: {e}"),
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AgtrsError::LlmCallFailed {
                reason: format!("Stream {status}: {text}"),
            });
        }

        let mut byte_stream = resp.bytes_stream();
        let s = stream! {
            use futures::StreamExt;
            let mut buf = String::new();
            // Maps stream index -> (tool_use_id, name)
            let mut tool_index_map: HashMap<usize, (String, String)> = HashMap::new();

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
                            output_tokens: u.completion_tokens,
                            cache_read_tokens: 0,
                            cache_write_tokens: 0,
                        });
                        let choice = match cd.choices.into_iter().next() {
                            Some(c) => c,
                            None => continue,
                        };
                        let stop_reason = Some(finish_to_stop(choice.finish_reason.as_deref()));

                        // Yield text delta
                        let text_delta = choice.delta.content.unwrap_or_default();
                        if !text_delta.is_empty() || choice.delta.tool_calls.is_empty() {
                            yield Ok(StreamChunk {
                                delta: text_delta,
                                thinking_delta: None,
                                tool_call_delta: None,
                                stop_reason: stop_reason.clone(),
                                usage: usage.clone(),
                            });
                        }

                        // Yield tool call deltas (one per tool call fragment)
                        for tc in choice.delta.tool_calls {
                            let idx = tc.index;
                            // Register id+name on first fragment for this index
                            if let Some(ref id) = tc.id {
                                let name = tc.function.name.clone().unwrap_or_default();
                                tool_index_map.entry(idx).or_insert_with(|| (id.clone(), name));
                            }
                            if let Some((tool_use_id, name)) = tool_index_map.get(&idx) {
                                let name_opt = tc.function.name.clone();
                                yield Ok(StreamChunk {
                                    delta: String::new(),
                                    thinking_delta: None,
                                    tool_call_delta: Some(ToolCallDelta {
                                        tool_use_id: tool_use_id.clone(),
                                        name: name_opt.or_else(|| {
                                            if tool_index_map.get(&idx).map(|(_, n)| !n.is_empty()).unwrap_or(false) {
                                                Some(name.clone())
                                            } else {
                                                None
                                            }
                                        }),
                                        input_delta: tc.function.arguments.clone(),
                                    }),
                                    stop_reason: stop_reason.clone(),
                                    usage: usage.clone(),
                                });
                            }
                        }
                    }
                }
            }
        };
        Ok(Box::pin(s))
    }

    async fn embed(
        &self,
        _inputs: &[String],
        _model: Option<&str>,
    ) -> Result<Vec<Embedding>, AgtrsError> {
        Err(AgtrsError::msg("Embeddings not supported"))
    }

    async fn count_tokens(&self, _messages: &[Message]) -> Result<usize, AgtrsError> {
        Err(AgtrsError::msg("Token counting not supported"))
    }

    fn context_window(&self) -> usize {
        128_000
    }
    fn model(&self) -> &str {
        &self.config.llm_model
    }
}
