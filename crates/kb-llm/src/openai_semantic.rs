use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use kb_core::domain::{KnowledgeChunk, SemanticProviderInfo};
use kb_core::{SemanticExtractRequest, SemanticExtractResponse, SemanticExtractor};
use serde::Deserialize;
use serde_json::{json, Value};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone)]
pub struct LlmSemanticConfig {
    pub key: Option<String>,
    pub model: String,
    pub base_url: String,
    pub semantic_flow: String,
    pub timeout_secs: u64,
    pub temperature: f64,
    pub max_output_tokens: u32,
    pub analysis_max_output_tokens: u32,
    pub max_input_chars: usize,
    pub max_chunks: usize,
    pub retry_max_attempts: usize,
    pub retry_backoff_ms: u64,
}

#[derive(Debug, Clone)]
pub struct LlmSemanticExtractor {
    config: LlmSemanticConfig,
    endpoint: String,
    provider_name: String,
}

impl LlmSemanticExtractor {
    pub fn new(config: LlmSemanticConfig) -> Self {
        let endpoint = build_chat_completion_endpoint(&config.base_url);
        let provider_name = format!("openai-compatible:{}@{}", config.model, config.base_url);
        Self {
            config,
            endpoint,
            provider_name,
        }
    }

    fn call_openai_chat_with_retry(
        &self,
        api_key: &str,
        system_prompt: &str,
        user_prompt: &str,
        max_output_tokens: u32,
    ) -> Result<Value, String> {
        let mut errors = Vec::new();
        for attempt in 1..=self.config.retry_max_attempts {
            match self.call_openai_chat_once(api_key, system_prompt, user_prompt, max_output_tokens)
            {
                Ok(value) => return Ok(value),
                Err(err) => {
                    errors.push(format!(
                        "attempt {}/{} failed: {}",
                        attempt, self.config.retry_max_attempts, err
                    ));
                    if attempt < self.config.retry_max_attempts && self.config.retry_backoff_ms > 0
                    {
                        std::thread::sleep(Duration::from_millis(self.config.retry_backoff_ms));
                    }
                }
            }
        }

        Err(format!(
            "llm request failed after {} attempts; {}",
            self.config.retry_max_attempts,
            errors.join(" | ")
        ))
    }

    fn call_openai_chat_once(
        &self,
        api_key: &str,
        system_prompt: &str,
        user_prompt: &str,
        max_output_tokens: u32,
    ) -> Result<Value, String> {
        let body = json!({
            "model": self.config.model,
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt
                },
                {
                    "role": "user",
                    "content": user_prompt
                }
            ],
            "temperature": self.config.temperature,
            "max_completion_tokens": max_output_tokens,
            "response_format": { "type": "json_object" }
        });
        match self.send_openai_chat(api_key, &body) {
            Ok(value) => Ok(value),
            Err(err) => {
                if supports_max_tokens_fallback(&err) {
                    let fallback_body = json!({
                        "model": self.config.model,
                        "messages": [
                            {
                                "role": "system",
                                "content": system_prompt
                            },
                            {
                                "role": "user",
                                "content": user_prompt
                            }
                        ],
                        "temperature": self.config.temperature,
                        "max_tokens": max_output_tokens,
                        "response_format": { "type": "json_object" }
                    });
                    self.send_openai_chat(api_key, &fallback_body)
                        .map_err(|fallback_err| {
                            format!(
                                "primary(max_completion_tokens) failed: {}; fallback(max_tokens) failed: {}",
                                err, fallback_err
                            )
                        })
                } else {
                    Err(err)
                }
            }
        }
    }

    fn send_openai_chat(&self, api_key: &str, body: &Value) -> Result<Value, String> {
        let request = ureq::post(&self.endpoint)
            .set("Authorization", &format!("Bearer {}", api_key))
            .set("Content-Type", "application/json")
            .timeout(Duration::from_secs(self.config.timeout_secs));

        let raw = match request.send_string(&body.to_string()) {
            Ok(response) => response
                .into_string()
                .map_err(|err| format!("failed to read llm response: {}", err))?,
            Err(ureq::Error::Status(code, response)) => {
                let body_text = response
                    .into_string()
                    .unwrap_or_else(|_| "<failed to read error body>".to_string());
                return Err(format!(
                    "http status {} from llm provider: {}",
                    code, body_text
                ));
            }
            Err(ureq::Error::Transport(err)) => {
                return Err(format!("llm transport error: {}", err));
            }
        };

        serde_json::from_str(&raw).map_err(|err| format!("invalid llm response json: {}", err))
    }

    fn extract_single_step(
        &self,
        api_key: &str,
        request: &SemanticExtractRequest,
        truncated_text: &str,
        planned_chunks: &[KnowledgeChunk],
    ) -> Result<LlmSemanticExecution, String> {
        let user_prompt =
            build_semantic_prompt_single_step(request, truncated_text, planned_chunks);
        let response = self.call_openai_chat_with_retry(
            api_key,
            "You are a semantic compiler for a local knowledge base. Return JSON only.",
            &user_prompt,
            self.config.max_output_tokens,
        )?;
        let content = extract_chat_message_content(&response)
            .ok_or_else(|| "missing choices[0].message.content in llm response".to_string())?;
        let payload_value = parse_json_object_from_text(&content)?;
        let payload = serde_json::from_value::<LlmSemanticPayload>(payload_value)
            .map_err(|err| format!("invalid llm semantic payload: {}", err))?;
        Ok(LlmSemanticExecution {
            payload,
            analysis: None,
        })
    }

    fn extract_two_step(
        &self,
        api_key: &str,
        request: &SemanticExtractRequest,
        truncated_text: &str,
        planned_chunks: &[KnowledgeChunk],
    ) -> Result<LlmSemanticExecution, String> {
        let analysis_prompt =
            build_semantic_prompt_analysis(request, truncated_text, planned_chunks);
        let analysis_response = self.call_openai_chat_with_retry(
            api_key,
            "You are an analysis engine for memory compilation. Return JSON only.",
            &analysis_prompt,
            self.config.analysis_max_output_tokens,
        )?;
        let analysis_content = extract_chat_message_content(&analysis_response)
            .ok_or_else(|| "missing choices[0].message.content in analysis response".to_string())?;
        let analysis_value = parse_json_object_from_text(&analysis_content)?;
        let analysis_payload =
            serde_json::from_value::<LlmSemanticAnalysisPayload>(analysis_value.clone())
                .map_err(|err| format!("invalid llm semantic analysis payload: {}", err))?;

        let generation_prompt =
            build_semantic_prompt_generation(request, planned_chunks, &analysis_value);
        let generation_response = self.call_openai_chat_with_retry(
            api_key,
            "You are a semantic compiler for a local knowledge base. Return JSON only.",
            &generation_prompt,
            self.config.max_output_tokens,
        )?;
        let generation_content =
            extract_chat_message_content(&generation_response).ok_or_else(|| {
                "missing choices[0].message.content in generation response".to_string()
            })?;
        let generation_value = parse_json_object_from_text(&generation_content)?;
        let payload = serde_json::from_value::<LlmSemanticPayload>(generation_value)
            .map_err(|err| format!("invalid llm semantic payload: {}", err))?;

        Ok(LlmSemanticExecution {
            payload,
            analysis: Some(analysis_payload),
        })
    }
}

impl SemanticExtractor for LlmSemanticExtractor {
    fn extract(&self, request: &SemanticExtractRequest) -> Result<SemanticExtractResponse, String> {
        let Some(api_key) = self.config.key.as_deref() else {
            return Err("llm semantic extraction requires KB_LLM_KEY".to_string());
        };

        let truncated_text = truncate_by_chars(&request.text, self.config.max_input_chars);
        let mut chunks =
            plan_semantic_chunks(&request.record_id, &truncated_text, self.config.max_chunks);
        if chunks.is_empty() {
            chunks.push(KnowledgeChunk {
                chunk_id: format!("{}:0001", request.record_id),
                byte_range: [0, truncated_text.len() as u64],
                content_signature: format!(
                    "fnv1a64:{:016x}",
                    fnv1a64_local(truncated_text.as_bytes())
                ),
                summary: stable_truncate(&truncated_text, 120),
                preview: stable_truncate(&truncated_text, 200),
            });
        }

        let execution = if self.config.semantic_flow == "single_step" {
            self.extract_single_step(api_key, request, &truncated_text, &chunks)?
        } else {
            self.extract_two_step(api_key, request, &truncated_text, &chunks)?
        };
        let LlmSemanticExecution { payload, analysis } = execution;
        let LlmSemanticPayload {
            summary: payload_summary,
            tags: payload_tags,
            key_concepts: payload_key_concepts,
            chunk_summaries,
        } = payload;

        let mut chunk_summary_map = BTreeMap::new();
        for item in chunk_summaries {
            let chunk_id = item.chunk_id.trim();
            let summary = item.summary.trim();
            if chunk_id.is_empty() || summary.is_empty() {
                continue;
            }
            chunk_summary_map.insert(chunk_id.to_string(), summary.to_string());
        }
        if chunk_summary_map.is_empty() {
            if let Some(analysis) = analysis.as_ref() {
                for insight in &analysis.chunk_insights {
                    let chunk_id = insight.chunk_id.trim();
                    if chunk_id.is_empty() {
                        continue;
                    }
                    if let Some(summary) = first_non_empty_list_item(&insight.facts) {
                        chunk_summary_map
                            .insert(chunk_id.to_string(), stable_truncate(summary, 120));
                    }
                }
            }
        }

        for chunk in &mut chunks {
            if let Some(summary) = chunk_summary_map.get(&chunk.chunk_id) {
                chunk.summary = stable_truncate(summary, 120);
            }
        }

        let fallback_summary = analysis
            .as_ref()
            .and_then(|analysis| normalize_text_field(analysis.summary.clone()))
            .or_else(|| first_non_empty_line_local(&truncated_text))
            .unwrap_or_else(|| format!("Content from {}", request.path));
        let summary = normalize_text_field(payload_summary).unwrap_or(fallback_summary);
        let tags = if payload_tags.is_empty() {
            analysis
                .as_ref()
                .map(|analysis| analysis.tags.clone())
                .unwrap_or_default()
        } else {
            payload_tags
        };
        let key_concepts = if payload_key_concepts.is_empty() {
            analysis
                .as_ref()
                .map(|analysis| analysis.key_concepts.clone())
                .unwrap_or_default()
        } else {
            payload_key_concepts
        };

        Ok(SemanticExtractResponse {
            summary: stable_truncate(&summary, 240),
            tags: normalize_semantic_list(tags, 12, true),
            key_concepts: normalize_semantic_list(key_concepts, 12, false),
            chunks,
            provider: SemanticProviderInfo {
                kind: "llm".to_string(),
                name: format!("{};flow={}", self.provider_name, self.config.semantic_flow),
            },
        })
    }
}

#[derive(Debug)]
struct LlmSemanticExecution {
    payload: LlmSemanticPayload,
    analysis: Option<LlmSemanticAnalysisPayload>,
}

#[derive(Debug, Deserialize)]
struct LlmSemanticPayload {
    summary: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    key_concepts: Vec<String>,
    #[serde(default)]
    chunk_summaries: Vec<LlmChunkSummary>,
}

#[derive(Debug, Deserialize)]
struct LlmChunkSummary {
    chunk_id: String,
    summary: String,
}

#[derive(Debug, Deserialize)]
struct LlmSemanticAnalysisPayload {
    summary: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    key_concepts: Vec<String>,
    #[serde(default)]
    chunk_insights: Vec<LlmChunkInsight>,
}

#[derive(Debug, Deserialize)]
struct LlmChunkInsight {
    chunk_id: String,
    #[serde(default)]
    facts: Vec<String>,
}

fn build_chat_completion_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{}/chat/completions", trimmed)
    }
}

fn build_semantic_prompt_single_step(
    request: &SemanticExtractRequest,
    truncated_text: &str,
    planned_chunks: &[KnowledgeChunk],
) -> String {
    let chunk_descriptors = planned_chunks
        .iter()
        .map(|chunk| {
            json!({
                "chunk_id": chunk.chunk_id,
                "preview": chunk.preview
            })
        })
        .collect::<Vec<_>>();
    let chunk_json =
        serde_json::to_string_pretty(&chunk_descriptors).unwrap_or_else(|_| "[]".to_string());

    format!(
        "Task: build semantic memory for one source file.\n\
Record ID: {record_id}\n\
Path: {path}\n\
\n\
SOURCE_TEXT:\n\
{source_text}\n\
\n\
CHUNKS:\n\
{chunk_json}\n\
\n\
Return JSON with exact keys:\n\
{{\n\
  \"summary\": \"one concise summary\",\n\
  \"tags\": [\"topic\", \"keyword\"],\n\
  \"key_concepts\": [\"Concept A\", \"Concept B\"],\n\
  \"chunk_summaries\": [{{\"chunk_id\": \"<id>\", \"summary\": \"short summary\"}}]\n\
}}\n\
Rules:\n\
- Output JSON only, no markdown.\n\
- Keep tags lowercase.\n\
- chunk_summaries must only reference chunk_id from CHUNKS.\n\
- Keep each chunk summary short and factual.",
        record_id = request.record_id,
        path = request.path,
        source_text = truncated_text,
        chunk_json = chunk_json
    )
}

fn build_semantic_prompt_analysis(
    request: &SemanticExtractRequest,
    truncated_text: &str,
    planned_chunks: &[KnowledgeChunk],
) -> String {
    let chunk_descriptors = planned_chunks
        .iter()
        .map(|chunk| {
            json!({
                "chunk_id": chunk.chunk_id,
                "preview": chunk.preview
            })
        })
        .collect::<Vec<_>>();
    let chunk_json =
        serde_json::to_string_pretty(&chunk_descriptors).unwrap_or_else(|_| "[]".to_string());

    format!(
        "Step 1 of 2: analyze one source file for semantic memory compilation.\n\
Record ID: {record_id}\n\
Path: {path}\n\
\n\
SOURCE_TEXT:\n\
{source_text}\n\
\n\
CHUNKS:\n\
{chunk_json}\n\
\n\
Return JSON with exact keys:\n\
{{\n\
  \"summary\": \"one concise source summary\",\n\
  \"tags\": [\"topic\", \"keyword\"],\n\
  \"key_concepts\": [\"Concept A\", \"Concept B\"],\n\
  \"chunk_insights\": [{{\"chunk_id\": \"<id>\", \"facts\": [\"fact1\", \"fact2\"]}}]\n\
}}\n\
Rules:\n\
- Output JSON only.\n\
- Keep tags lowercase.\n\
- chunk_insights must only reference chunk_id from CHUNKS.\n\
- Facts should be short and factual.",
        record_id = request.record_id,
        path = request.path,
        source_text = truncated_text,
        chunk_json = chunk_json
    )
}

fn build_semantic_prompt_generation(
    request: &SemanticExtractRequest,
    planned_chunks: &[KnowledgeChunk],
    analysis_value: &Value,
) -> String {
    let chunk_descriptors = planned_chunks
        .iter()
        .map(|chunk| {
            json!({
                "chunk_id": chunk.chunk_id,
                "preview": chunk.preview
            })
        })
        .collect::<Vec<_>>();
    let chunk_json =
        serde_json::to_string_pretty(&chunk_descriptors).unwrap_or_else(|_| "[]".to_string());
    let analysis_json =
        serde_json::to_string_pretty(analysis_value).unwrap_or_else(|_| "{}".to_string());

    format!(
        "Step 2 of 2: generate compile-ready semantic memory from analysis.\n\
Record ID: {record_id}\n\
Path: {path}\n\
\n\
CHUNKS:\n\
{chunk_json}\n\
\n\
ANALYSIS_JSON:\n\
{analysis_json}\n\
\n\
Return JSON with exact keys:\n\
{{\n\
  \"summary\": \"one concise summary\",\n\
  \"tags\": [\"topic\", \"keyword\"],\n\
  \"key_concepts\": [\"Concept A\", \"Concept B\"],\n\
  \"chunk_summaries\": [{{\"chunk_id\": \"<id>\", \"summary\": \"short summary\"}}]\n\
}}\n\
Rules:\n\
- Output JSON only, no markdown.\n\
- Keep tags lowercase.\n\
- chunk_summaries must only reference chunk_id from CHUNKS.\n\
- Keep each chunk summary short and factual.",
        record_id = request.record_id,
        path = request.path,
        chunk_json = chunk_json,
        analysis_json = analysis_json
    )
}

fn extract_chat_message_content(response: &Value) -> Option<String> {
    let message_content = response.pointer("/choices/0/message/content")?;
    if let Some(text) = message_content.as_str() {
        return Some(text.to_string());
    }
    if let Some(parts) = message_content.as_array() {
        let joined = parts
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("");
        if !joined.trim().is_empty() {
            return Some(joined);
        }
    }
    None
}

fn parse_json_object_from_text(text: &str) -> Result<Value, String> {
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        return Ok(value);
    }

    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            if end > start {
                let candidate = &text[start..=end];
                if let Ok(value) = serde_json::from_str::<Value>(candidate) {
                    return Ok(value);
                }
            }
        }
    }

    Err("llm content is not a valid json object".to_string())
}

fn supports_max_tokens_fallback(error_text: &str) -> bool {
    let lowered = error_text.to_ascii_lowercase();
    lowered.contains("max_completion_tokens")
        || lowered.contains("unknown field")
        || lowered.contains("unrecognized field")
        || lowered.contains("invalid request")
        || lowered.contains("unexpected field")
}

fn first_non_empty_list_item(items: &[String]) -> Option<&str> {
    items
        .iter()
        .map(|item| item.trim())
        .find(|item| !item.is_empty())
}

fn truncate_by_chars(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    input.chars().take(max_chars).collect::<String>()
}

fn plan_semantic_chunks(record_id: &str, text: &str, max_chunks: usize) -> Vec<KnowledgeChunk> {
    if max_chunks == 0 || text.trim().is_empty() {
        return vec![];
    }

    let mut chunks = Vec::new();
    let mut cursor = 0usize;
    for raw_part in text.split("\n\n") {
        if chunks.len() >= max_chunks {
            break;
        }

        let local_trimmed = raw_part.trim();
        if local_trimmed.is_empty() {
            cursor = cursor.saturating_add(raw_part.len() + 2);
            continue;
        }

        let local_offset = raw_part.find(local_trimmed).unwrap_or(0);
        let start = cursor.saturating_add(local_offset);
        let end = start.saturating_add(local_trimmed.len()).min(text.len());
        let chunk_index = chunks.len() + 1;

        chunks.push(KnowledgeChunk {
            chunk_id: format!("{}:{:04}", record_id, chunk_index),
            byte_range: [start as u64, end as u64],
            content_signature: format!("fnv1a64:{:016x}", fnv1a64_local(local_trimmed.as_bytes())),
            summary: stable_truncate(local_trimmed, 120),
            preview: stable_truncate(local_trimmed, 200),
        });

        cursor = cursor.saturating_add(raw_part.len() + 2);
    }

    if chunks.is_empty() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            chunks.push(KnowledgeChunk {
                chunk_id: format!("{}:0001", record_id),
                byte_range: [0, trimmed.len() as u64],
                content_signature: format!("fnv1a64:{:016x}", fnv1a64_local(trimmed.as_bytes())),
                summary: stable_truncate(trimmed, 120),
                preview: stable_truncate(trimmed, 200),
            });
        }
    }

    chunks
}

fn normalize_text_field(input: Option<String>) -> Option<String> {
    input.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_semantic_list(values: Vec<String>, limit: usize, lowercase: bool) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for raw in values {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let canonical = if lowercase {
            trimmed.to_ascii_lowercase()
        } else {
            trimmed.to_string()
        };
        if seen.insert(canonical.clone()) {
            out.push(canonical);
        }
        if out.len() >= limit {
            break;
        }
    }
    out
}

fn first_non_empty_line_local(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToString::to_string)
}

fn fnv1a64_local(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn stable_truncate(text: &str, max_chars: usize) -> String {
    let grapheme_starts: Vec<usize> = UnicodeSegmentation::grapheme_indices(text, true)
        .map(|(idx, _)| idx)
        .collect();
    if grapheme_starts.len() <= max_chars {
        return text.to_string();
    }
    let end_b = grapheme_starts
        .get(max_chars)
        .copied()
        .unwrap_or(text.len());
    let mut out = text[..end_b].to_string();
    out.push_str("...");
    out
}
