use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use crate::agent_tools::{SemanticAgentContext, SemanticAgentToolbox};
use crate::protocol::{
    self, build_chat_completion_endpoint, extract_chat_message_content, normalize_text_field,
    parse_json_object_from_text, ChatProtocolSettings,
};
use kb_core::domain::{KnowledgeChunk, SemanticProviderInfo};
use kb_core::error::DomainError;
use kb_core::usecases::fnv1a64;
use kb_core::{
    split_paragraph_spans, SemanticExtractRequest, SemanticExtractResponse, SemanticExtractor,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone)]
pub struct LlmSemanticConfig {
    pub key: Option<String>,
    pub model: String,
    pub base_url: String,
    pub workspace_root: Option<PathBuf>,
    pub progress: Option<Arc<dyn Fn(String) + Send + Sync>>,
    pub semantic_flow: String,
    pub timeout_secs: u64,
    pub temperature: f64,
    pub max_output_tokens: u32,
    pub analysis_max_output_tokens: u32,
    pub max_input_chars: usize,
    pub max_chunks: usize,
    pub retry_max_attempts: usize,
    pub retry_backoff_ms: u64,
    pub agent_loop_max_rounds: usize,
    pub agent_reflection_enabled: bool,
    pub agent_min_chunk_coverage: f64,
}

#[derive(Clone)]
pub struct LlmSemanticExtractor {
    config: LlmSemanticConfig,
    endpoint: String,
    provider_name: String,
    toolbox: SemanticAgentToolbox,
}

impl LlmSemanticExtractor {
    pub fn new(config: LlmSemanticConfig) -> Self {
        let endpoint = build_chat_completion_endpoint(&config.base_url);
        let provider_name = format!("openai-compatible:{}@{}", config.model, config.base_url);
        let toolbox = SemanticAgentToolbox::new(config.workspace_root.clone());
        Self {
            config,
            endpoint,
            provider_name,
            toolbox,
        }
    }

    fn call_openai_chat_with_retry(
        &self,
        api_key: &str,
        system_prompt: &str,
        user_prompt: &str,
        max_output_tokens: u32,
    ) -> Result<Value, String> {
        // 注意：此处沿用既有语义，把 retry_max_attempts 原值传入（不做 .max(1)
        // 钳制）——与 openai_graph_wiki 的调用点保持各自原有行为。
        protocol::call_openai_chat_with_retry(
            &ChatProtocolSettings {
                endpoint: &self.endpoint,
                api_key,
                timeout_secs: self.config.timeout_secs,
                model: &self.config.model,
                temperature: self.config.temperature,
                max_output_tokens,
            },
            self.config.retry_max_attempts,
            self.config.retry_backoff_ms,
            system_prompt,
            user_prompt,
        )
    }

    fn emit_progress(&self, stage: &str, request: &SemanticExtractRequest, detail: Option<String>) {
        if let Some(callback) = &self.config.progress {
            callback(format_semantic_progress(
                stage,
                &request.record_id,
                &request.path,
                detail,
            ));
        }
    }

    fn extract_single_step(
        &self,
        api_key: &str,
        request: &SemanticExtractRequest,
        truncated_text: &str,
        planned_chunks: &[KnowledgeChunk],
        agent_context: &SemanticAgentContext,
    ) -> Result<LlmSemanticExecution, String> {
        self.emit_progress(
            "single-step-start",
            request,
            Some(format!("chunks={}", planned_chunks.len())),
        );
        let user_prompt = build_semantic_prompt_single_step(
            request,
            truncated_text,
            planned_chunks,
            agent_context,
        );
        let response = self.call_openai_chat_with_retry(
            api_key,
            "You are a semantic compiler for a local knowledge base. Return JSON only.",
            &user_prompt,
            self.config.max_output_tokens,
        )?;
        let payload = decode_semantic_payload(&response, "llm response")?;
        self.emit_progress(
            "single-step-ok",
            request,
            Some(format!("chunk_summaries={}", payload.chunk_summaries.len())),
        );
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
        agent_context: &SemanticAgentContext,
    ) -> Result<LlmSemanticExecution, String> {
        self.emit_progress(
            "analysis-start",
            request,
            Some(format!("chunks={}", planned_chunks.len())),
        );
        let analysis_prompt =
            build_semantic_prompt_analysis(request, truncated_text, planned_chunks, agent_context);
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
        self.emit_progress(
            "analysis-ok",
            request,
            Some(format!(
                "chunk_insights={} tags={} concepts={}",
                analysis_payload.chunk_insights.len(),
                analysis_payload.tags.len(),
                analysis_payload.key_concepts.len()
            )),
        );

        let generation_prompt = build_semantic_prompt_generation(
            request,
            planned_chunks,
            &analysis_value,
            agent_context,
        );
        self.emit_progress("generation-start", request, None);
        let generation_response = self.call_openai_chat_with_retry(
            api_key,
            "You are a semantic compiler for a local knowledge base. Return JSON only.",
            &generation_prompt,
            self.config.max_output_tokens,
        )?;
        let payload = decode_semantic_payload(&generation_response, "generation response")?;
        self.emit_progress(
            "generation-ok",
            request,
            Some(format!("chunk_summaries={}", payload.chunk_summaries.len())),
        );

        Ok(LlmSemanticExecution {
            payload,
            analysis: Some(analysis_payload),
        })
    }

    fn run_reflection_loop(
        &self,
        api_key: &str,
        request: &SemanticExtractRequest,
        planned_chunks: &[KnowledgeChunk],
        agent_context: &SemanticAgentContext,
        mut execution: LlmSemanticExecution,
    ) -> Result<LlmSemanticExecution, String> {
        let max_rounds = self.config.agent_loop_max_rounds.clamp(1, 4);
        if !self.config.agent_reflection_enabled || max_rounds <= 1 {
            return Ok(execution);
        }

        let min_coverage = self.config.agent_min_chunk_coverage.clamp(0.0, 1.0);
        for round in 2..=max_rounds {
            let coverage = calculate_chunk_coverage(&execution.payload, planned_chunks);
            if is_payload_good_enough(&execution.payload, coverage, min_coverage) {
                self.emit_progress(
                    "reflection-skip",
                    request,
                    Some(format!("round={} coverage={:.3}", round, coverage)),
                );
                break;
            }

            let missing_chunk_ids = collect_missing_chunk_ids(&execution.payload, planned_chunks);
            self.emit_progress(
                "reflection-start",
                request,
                Some(format!(
                    "round={} coverage={:.3} missing={}",
                    round,
                    coverage,
                    if missing_chunk_ids.is_empty() {
                        "(none)".to_string()
                    } else {
                        missing_chunk_ids.join(",")
                    }
                )),
            );
            let user_prompt = build_semantic_prompt_reflection(
                request,
                planned_chunks,
                agent_context,
                &execution.payload,
                execution.analysis.as_ref(),
                coverage,
                min_coverage,
                round,
                &missing_chunk_ids,
            );
            let reflection_response = self.call_openai_chat_with_retry(
                api_key,
                "You are a semantic memory verifier and repair engine. Return JSON only.",
                &user_prompt,
                self.config.max_output_tokens,
            )?;
            let reflected = decode_semantic_payload(&reflection_response, "reflection response")?;
            execution.payload = merge_semantic_payloads(&execution.payload, &reflected);
            let next_coverage = calculate_chunk_coverage(&execution.payload, planned_chunks);
            self.emit_progress(
                "reflection-ok",
                request,
                Some(format!("round={} coverage={:.3}", round, next_coverage)),
            );
        }

        Ok(execution)
    }
}

impl SemanticExtractor for LlmSemanticExtractor {
    fn extract(
        &self,
        request: &SemanticExtractRequest,
    ) -> Result<SemanticExtractResponse, DomainError> {
        let Some(api_key) = self.config.key.as_deref() else {
            return Err(DomainError::config(
                "llm semantic extraction requires KB_LLM_KEY",
            ));
        };
        self.emit_progress("record-start", request, None);

        let truncated_text = truncate_by_chars(&request.text, self.config.max_input_chars);
        let mut chunks =
            plan_semantic_chunks(&request.record_id, &truncated_text, self.config.max_chunks);
        if chunks.is_empty() {
            chunks.push(KnowledgeChunk {
                chunk_id: format!("{}:0001", request.record_id),
                byte_range: [0, truncated_text.len() as u64],
                content_signature: format!(
                    "fnv1a64:{:016x}",
                    fnv1a64(truncated_text.as_bytes())
                ),
                summary: stable_truncate(&truncated_text, 120),
                preview: stable_truncate(&truncated_text, 200),
            });
        }

        let agent_context = self.toolbox.build_context(request, &truncated_text);

        let execution = if self.config.semantic_flow == "single_step" {
            self.extract_single_step(api_key, request, &truncated_text, &chunks, &agent_context)
                .map_err(DomainError::external)?
        } else {
            self.extract_two_step(api_key, request, &truncated_text, &chunks, &agent_context)
                .map_err(DomainError::external)?
        };
        let execution = self
            .run_reflection_loop(api_key, request, &chunks, &agent_context, execution)
            .map_err(DomainError::external)?;
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
        .inspect(|response| {
            self.emit_progress(
                "record-done",
                request,
                Some(format!(
                    "tags={} concepts={} chunks={}",
                    response.tags.len(),
                    response.key_concepts.len(),
                    response.chunks.len()
                )),
            );
        })
    }

    /// 并行批处理：在共享 tokio runtime 上用 semaphore 限制并发地跑 `extract`。
    ///
    /// - 每条 request 走一个 `spawn_blocking` 任务（因为 `extract` 内部仍是
    ///   阻塞 `ureq` 调用 + CPU 解析），避免堵塞 runtime worker。
    /// - 并发上限取 `KB_LLM_MAX_PARALLEL`，缺省 4，clamp 到 [1, 32]。
    ///   超过 32 没意义——所有主流 OpenAI 兼容 provider 都在更小的范围内 rate-limit。
    /// - 顺序：返回值与 requests 一一对应，靠 `tokio::spawn` 句柄按收集顺序 await。
    fn extract_batch(
        &self,
        requests: &[SemanticExtractRequest],
    ) -> Vec<Result<SemanticExtractResponse, DomainError>> {
        if requests.is_empty() {
            return Vec::new();
        }
        let max_parallel = std::env::var("KB_LLM_MAX_PARALLEL")
            .ok()
            .and_then(|raw| raw.trim().parse::<usize>().ok())
            .filter(|n| *n >= 1)
            .unwrap_or(4)
            .min(32);

        let extractor = Arc::new(self.clone());
        let owned_requests: Vec<SemanticExtractRequest> = requests.to_vec();

        crate::http::block_on(async move {
            let semaphore = Arc::new(tokio::sync::Semaphore::new(max_parallel));
            let mut handles = Vec::with_capacity(owned_requests.len());
            for request in owned_requests {
                let extractor_clone = Arc::clone(&extractor);
                let semaphore_clone = Arc::clone(&semaphore);
                handles.push(tokio::spawn(async move {
                    let _permit = semaphore_clone
                        .acquire_owned()
                        .await
                        .expect("kb-llm semaphore closed unexpectedly");
                    tokio::task::spawn_blocking(move || extractor_clone.extract(&request))
                        .await
                        .unwrap_or_else(|err| {
                            Err(DomainError::external(format!(
                                "kb-llm parallel extract panicked: {err}"
                            )))
                        })
                }));
            }

            let mut results = Vec::with_capacity(handles.len());
            for handle in handles {
                let res = handle.await.unwrap_or_else(|err| {
                    Err(DomainError::external(format!(
                        "kb-llm parallel join failed: {err}"
                    )))
                });
                results.push(res);
            }
            results
        })
    }
}

#[derive(Debug)]
struct LlmSemanticExecution {
    payload: LlmSemanticPayload,
    analysis: Option<LlmSemanticAnalysisPayload>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct LlmSemanticPayload {
    summary: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    key_concepts: Vec<String>,
    #[serde(default)]
    chunk_summaries: Vec<LlmChunkSummary>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct LlmChunkSummary {
    chunk_id: String,
    summary: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct LlmSemanticAnalysisPayload {
    summary: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    key_concepts: Vec<String>,
    #[serde(default)]
    chunk_insights: Vec<LlmChunkInsight>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct LlmChunkInsight {
    chunk_id: String,
    #[serde(default)]
    facts: Vec<String>,
}

fn build_semantic_prompt_single_step(
    request: &SemanticExtractRequest,
    truncated_text: &str,
    planned_chunks: &[KnowledgeChunk],
    agent_context: &SemanticAgentContext,
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
    let context_json =
        serde_json::to_string_pretty(agent_context).unwrap_or_else(|_| "{}".to_string());

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
ATOMIC_TOOLS_AVAILABLE:\n\
- read_raw_file(path)\n\
- read_knowledge_graph(record_id)\n\
- read_file_summary(record_id_or_path)\n\
\n\
ATOMIC_TOOL_CONTEXT:\n\
{context_json}\n\
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
- Keep each chunk summary short and factual.\n\
- When ATOMIC_TOOL_CONTEXT has useful clues, align summary/tags/concepts with it.",
        record_id = request.record_id,
        path = request.path,
        source_text = truncated_text,
        chunk_json = chunk_json,
        context_json = context_json
    )
}

fn build_semantic_prompt_analysis(
    request: &SemanticExtractRequest,
    truncated_text: &str,
    planned_chunks: &[KnowledgeChunk],
    agent_context: &SemanticAgentContext,
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
    let context_json =
        serde_json::to_string_pretty(agent_context).unwrap_or_else(|_| "{}".to_string());

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
ATOMIC_TOOLS_AVAILABLE:\n\
- read_raw_file(path)\n\
- read_knowledge_graph(record_id)\n\
- read_file_summary(record_id_or_path)\n\
\n\
ATOMIC_TOOL_CONTEXT:\n\
{context_json}\n\
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
- Facts should be short and factual.\n\
- If the graph or summaries indicate dependencies, include them in key_concepts/facts.",
        record_id = request.record_id,
        path = request.path,
        source_text = truncated_text,
        chunk_json = chunk_json,
        context_json = context_json
    )
}

fn build_semantic_prompt_generation(
    request: &SemanticExtractRequest,
    planned_chunks: &[KnowledgeChunk],
    analysis_value: &Value,
    agent_context: &SemanticAgentContext,
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
    let context_json =
        serde_json::to_string_pretty(agent_context).unwrap_or_else(|_| "{}".to_string());

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
ATOMIC_TOOL_CONTEXT:\n\
{context_json}\n\
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
- Keep each chunk summary short and factual.\n\
- Reconcile the final summary with ANALYSIS_JSON and ATOMIC_TOOL_CONTEXT.",
        record_id = request.record_id,
        path = request.path,
        chunk_json = chunk_json,
        analysis_json = analysis_json,
        context_json = context_json
    )
}

// Reflection prompt 拼接需要把所有 round-N 上下文（请求、已规划 chunk、
// agent 工具上下文、上一轮 payload、分析 payload、覆盖率、最小阈值、当前轮次、
// 缺失 chunk 列表）全部送进模板。引入参数结构体只会徒增包装/解包样板，
// 因此此处显式抑制 too_many_arguments。
#[allow(clippy::too_many_arguments)]
fn build_semantic_prompt_reflection(
    request: &SemanticExtractRequest,
    planned_chunks: &[KnowledgeChunk],
    agent_context: &SemanticAgentContext,
    prior_payload: &LlmSemanticPayload,
    analysis_payload: Option<&LlmSemanticAnalysisPayload>,
    current_coverage: f64,
    min_coverage: f64,
    round: usize,
    missing_chunk_ids: &[String],
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
    let prior_payload_json =
        serde_json::to_string_pretty(prior_payload).unwrap_or_else(|_| "{}".to_string());
    let analysis_json = analysis_payload
        .and_then(|analysis| serde_json::to_string_pretty(analysis).ok())
        .unwrap_or_else(|| "null".to_string());
    let missing_json =
        serde_json::to_string_pretty(missing_chunk_ids).unwrap_or_else(|_| "[]".to_string());
    let context_json =
        serde_json::to_string_pretty(agent_context).unwrap_or_else(|_| "{}".to_string());

    format!(
        "Agent reflection round {round}: verify and repair semantic memory output.\n\
Record ID: {record_id}\n\
Path: {path}\n\
Current chunk coverage: {current_coverage:.3}\n\
Required minimum coverage: {min_coverage:.3}\n\
\n\
CHUNKS:\n\
{chunk_json}\n\
\n\
PREVIOUS_OUTPUT:\n\
{prior_payload_json}\n\
\n\
ANALYSIS_JSON:\n\
{analysis_json}\n\
\n\
ATOMIC_TOOL_CONTEXT:\n\
{context_json}\n\
\n\
MISSING_CHUNK_IDS:\n\
{missing_json}\n\
\n\
Return JSON with exact keys:\n\
{{\n\
  \"summary\": \"one concise summary\",\n\
  \"tags\": [\"topic\", \"keyword\"],\n\
  \"key_concepts\": [\"Concept A\", \"Concept B\"],\n\
  \"chunk_summaries\": [{{\"chunk_id\": \"<id>\", \"summary\": \"short summary\"}}]\n\
}}\n\
Rules:\n\
- Output JSON only.\n\
- Keep tags lowercase.\n\
- chunk_summaries must only reference chunk_id from CHUNKS.\n\
- Prioritize filling missing chunk coverage first.\n\
- Keep each chunk summary short and factual.\n\
- Use ATOMIC_TOOL_CONTEXT to repair weak or inconsistent points.",
        round = round,
        record_id = request.record_id,
        path = request.path,
        current_coverage = current_coverage,
        min_coverage = min_coverage,
        chunk_json = chunk_json,
        prior_payload_json = prior_payload_json,
        analysis_json = analysis_json,
        context_json = context_json,
        missing_json = missing_json
    )
}

fn decode_semantic_payload(response: &Value, label: &str) -> Result<LlmSemanticPayload, String> {
    let content = extract_chat_message_content(response)
        .ok_or_else(|| format!("missing choices[0].message.content in {}", label))?;
    let payload_value = parse_json_object_from_text(&content)?;
    serde_json::from_value::<LlmSemanticPayload>(payload_value)
        .map_err(|err| format!("invalid llm semantic payload from {}: {}", label, err))
}

fn merge_semantic_payloads(
    prior: &LlmSemanticPayload,
    patch: &LlmSemanticPayload,
) -> LlmSemanticPayload {
    let summary = normalize_text_field(patch.summary.clone())
        .or_else(|| normalize_text_field(prior.summary.clone()));

    let tags = if patch.tags.is_empty() {
        prior.tags.clone()
    } else {
        normalize_semantic_list(
            prior
                .tags
                .iter()
                .chain(patch.tags.iter())
                .cloned()
                .collect::<Vec<_>>(),
            16,
            true,
        )
    };
    let key_concepts = if patch.key_concepts.is_empty() {
        prior.key_concepts.clone()
    } else {
        normalize_semantic_list(
            prior
                .key_concepts
                .iter()
                .chain(patch.key_concepts.iter())
                .cloned()
                .collect::<Vec<_>>(),
            16,
            false,
        )
    };

    let mut summaries = BTreeMap::<String, String>::new();
    for item in &prior.chunk_summaries {
        let chunk_id = item.chunk_id.trim();
        let summary_text = item.summary.trim();
        if !chunk_id.is_empty() && !summary_text.is_empty() {
            summaries.insert(chunk_id.to_string(), summary_text.to_string());
        }
    }
    for item in &patch.chunk_summaries {
        let chunk_id = item.chunk_id.trim();
        let summary_text = item.summary.trim();
        if !chunk_id.is_empty() && !summary_text.is_empty() {
            summaries.insert(chunk_id.to_string(), summary_text.to_string());
        }
    }
    let chunk_summaries = summaries
        .into_iter()
        .map(|(chunk_id, summary_text)| LlmChunkSummary {
            chunk_id,
            summary: summary_text,
        })
        .collect::<Vec<_>>();

    LlmSemanticPayload {
        summary,
        tags,
        key_concepts,
        chunk_summaries,
    }
}

fn calculate_chunk_coverage(
    payload: &LlmSemanticPayload,
    planned_chunks: &[KnowledgeChunk],
) -> f64 {
    if planned_chunks.is_empty() {
        return 1.0;
    }
    let allowed_ids = planned_chunks
        .iter()
        .map(|chunk| chunk.chunk_id.as_str())
        .collect::<BTreeSet<_>>();
    let covered = payload
        .chunk_summaries
        .iter()
        .map(|item| item.chunk_id.trim())
        .filter(|chunk_id| allowed_ids.contains(chunk_id))
        .collect::<BTreeSet<_>>()
        .len();
    covered as f64 / planned_chunks.len() as f64
}

fn collect_missing_chunk_ids(
    payload: &LlmSemanticPayload,
    planned_chunks: &[KnowledgeChunk],
) -> Vec<String> {
    let covered = payload
        .chunk_summaries
        .iter()
        .map(|item| item.chunk_id.trim())
        .collect::<BTreeSet<_>>();
    planned_chunks
        .iter()
        .filter(|chunk| !covered.contains(chunk.chunk_id.as_str()))
        .map(|chunk| chunk.chunk_id.clone())
        .collect()
}

fn is_payload_good_enough(payload: &LlmSemanticPayload, coverage: f64, min_coverage: f64) -> bool {
    normalize_text_field(payload.summary.clone()).is_some() && coverage >= min_coverage
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

    // 段落切分统一走 kb-core 的共享实现：按空行（兼容 CRLF）切分，
    // 返回指向原文的精确字节区间，天然规避 "\n\n" 匹配失败与游标漂移问题。
    let mut chunks = Vec::new();
    for (start, end) in split_paragraph_spans(text) {
        if chunks.len() >= max_chunks {
            break;
        }

        let chunk_text = &text[start..end];
        let chunk_index = chunks.len() + 1;

        chunks.push(KnowledgeChunk {
            chunk_id: format!("{}:{:04}", record_id, chunk_index),
            byte_range: [start as u64, end as u64],
            content_signature: format!("fnv1a64:{:016x}", fnv1a64(chunk_text.as_bytes())),
            summary: stable_truncate(chunk_text, 120),
            preview: stable_truncate(chunk_text, 200),
        });
    }

    if chunks.is_empty() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            chunks.push(KnowledgeChunk {
                chunk_id: format!("{}:0001", record_id),
                byte_range: [0, trimmed.len() as u64],
                content_signature: format!("fnv1a64:{:016x}", fnv1a64(trimmed.as_bytes())),
                summary: stable_truncate(trimmed, 120),
                preview: stable_truncate(trimmed, 200),
            });
        }
    }

    chunks
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

fn format_semantic_progress(
    stage: &str,
    record_id: &str,
    path: &str,
    detail: Option<String>,
) -> String {
    let mut parts = vec![
        "layer=semantic".to_string(),
        format!("stage={}", stage),
        format!("record={}", record_id),
        format!("path={}", path),
    ];
    if let Some(detail) = detail {
        let trimmed = detail.trim();
        if !trimmed.is_empty() {
            parts.push(trimmed.to_string());
        }
    }
    parts.join(" | ")
}

fn first_non_empty_line_local(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToString::to_string)
}

/// 字素簇（grapheme cluster）级稳定截断。
///
/// 与 `crate::protocol::stable_truncate`（char 级）**有意**不同：本文件的截断
/// 结果会写入用户可见的 chunk `summary`/`preview` 与最终语义摘要，按字素簇
/// 截断可避免把 emoji/组合字符从中间切开。合并到共享模块会改变既有输出，
/// 因此 Stage 5 去重时保留本地变体（见 protocol.rs 模块文档）。
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

#[cfg(test)]
mod tests {
    use super::{fnv1a64, format_semantic_progress, plan_semantic_chunks};

    // 说明：parse_json_object_from_text / supports_max_tokens_fallback 等共享
    // 协议助手的测试已随实现迁移至 crate::protocol::tests。

    #[test]
    fn semantic_progress_mentions_record_and_stage() {
        let line = format_semantic_progress(
            "analysis-start",
            "docs/config",
            "./docs/config.md",
            Some("chunks=3".to_string()),
        );
        assert!(line.contains("analysis-start"));
        assert!(line.contains("docs/config"));
        assert!(line.contains("./docs/config.md"));
        assert!(line.contains("chunks=3"));
    }

    #[test]
    fn plan_semantic_chunks_splits_crlf_paragraphs_with_real_offsets() {
        let text = "Para one.\r\n\r\nPara two.\r\n";
        let chunks = plan_semantic_chunks("docs/a", text, 6);

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].chunk_id, "docs/a:0001");
        assert_eq!(chunks[0].byte_range, [0, 9]);
        assert_eq!(
            chunks[0].content_signature,
            format!("fnv1a64:{:016x}", fnv1a64("Para one.".as_bytes()))
        );
        assert_eq!(chunks[1].chunk_id, "docs/a:0002");
        assert_eq!(chunks[1].byte_range, [13, 22]);
        assert_eq!(
            chunks[1].content_signature,
            format!("fnv1a64:{:016x}", fnv1a64("Para two.".as_bytes()))
        );
        assert_eq!(&text[13..22], "Para two.");
    }

    #[test]
    fn plan_semantic_chunks_respects_max_chunks_and_triple_newlines() {
        let text = "a\n\n\nb\n\nc";
        let chunks = plan_semantic_chunks("docs/a", text, 2);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].byte_range, [0, 1]);
        assert_eq!(chunks[1].byte_range, [4, 5]);
        assert_eq!(&text[4..5], "b");
    }

    #[test]
    fn plan_semantic_chunks_returns_empty_for_blank_text_or_zero_budget() {
        assert!(plan_semantic_chunks("docs/a", "  \r\n  ", 4).is_empty());
        assert!(plan_semantic_chunks("docs/a", "content", 0).is_empty());
    }
}
