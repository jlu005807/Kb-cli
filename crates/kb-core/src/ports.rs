//! Ports 定义：核心层对外依赖抽象。
//! 通过 trait 隔离存储、检索与应答实现，保证 usecase 可测试与可替换。
//!
//! 所有 port 与桥接器统一使用 [`DomainError`]，让消费端能按 io / config /
//! external 等语义维度分流（重试 / 终止 / 用户提示），而不是依赖字符串匹配。

use crate::domain::{
    GraphSnapshot, IndexSnapshot, KnowledgeChunk, KnowledgeSnapshot, SearchProjectionSnapshot,
    SemanticProviderInfo, WikiManifestSnapshot, WikiViewSnapshot,
};
use crate::error::DomainError;

// 运行态引用视图：供 usecase 层消费，不暴露具体持久化细节。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateRef {
    pub alias: String,
    pub path: String,
    pub category: String,
    pub perms: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StateSnapshot {
    pub refs: Vec<StateRef>,
}

// 状态端口：抽象“引用状态”的加载与保存。
pub trait StateStore {
    fn load_state(&self) -> Result<StateSnapshot, DomainError>;
    fn save_state(&self, state: &StateSnapshot) -> Result<(), DomainError>;
}

// 索引端口：抽象“索引快照”的读取与持久化。
pub trait IndexStore {
    fn read_index(&self) -> Result<Option<IndexSnapshot>, DomainError>;
    fn write_index(&self, snapshot: &IndexSnapshot) -> Result<(), DomainError>;
}

pub trait KnowledgeStore {
    fn read_knowledge(&self) -> Result<Option<KnowledgeSnapshot>, DomainError>;
    fn write_knowledge(&self, snapshot: &KnowledgeSnapshot) -> Result<(), DomainError>;
}

pub trait SearchStore {
    fn read_search(&self) -> Result<Option<SearchProjectionSnapshot>, DomainError>;
    fn write_search(&self, snapshot: &SearchProjectionSnapshot) -> Result<(), DomainError>;
}

pub trait GraphStore {
    fn read_graph(&self) -> Result<Option<GraphSnapshot>, DomainError>;
    fn write_graph(&self, snapshot: &GraphSnapshot) -> Result<(), DomainError>;
}

pub trait WikiStore {
    fn read_wiki_manifest(&self) -> Result<Option<WikiManifestSnapshot>, DomainError>;
    fn write_wiki_view(
        &self,
        snapshot: &WikiViewSnapshot,
        removed_paths: &[String],
    ) -> Result<(), DomainError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticExtractRequest {
    pub record_id: String,
    pub path: String,
    pub text: String,
    pub input_signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticExtractResponse {
    pub summary: String,
    pub tags: Vec<String>,
    pub key_concepts: Vec<String>,
    pub chunks: Vec<KnowledgeChunk>,
    pub provider: SemanticProviderInfo,
}

pub trait SemanticExtractor {
    fn extract(
        &self,
        request: &SemanticExtractRequest,
    ) -> Result<SemanticExtractResponse, DomainError>;
}

pub trait FileContentReader {
    fn read_text(&self, path: &str) -> Result<String, DomainError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetrievedContext {
    pub alias: String,
    pub snippet: String,
    pub score: f64,
    pub source_layer: Option<String>,
    pub record_id: Option<String>,
    pub chunk_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnswerMeta {
    pub engine_status: String,
    pub provider: Option<String>,
    pub trace_id: Option<String>,
}

// 检索端口：负责从索引中召回上下文，不关心最终生成答案。
pub trait Retriever {
    fn retrieve(
        &self,
        question: &str,
        top: usize,
        snapshot: &IndexSnapshot,
    ) -> Result<Vec<RetrievedContext>, DomainError>;
}

// 应答端口：负责产出最终应答元信息（当前实现为 disabled 占位）。
pub trait AnswerEngine {
    fn meta(
        &self,
        question: &str,
        contexts: &[RetrievedContext],
    ) -> Result<AnswerMeta, DomainError>;
}

// 后端知识库检索请求：用于接入远端/服务化知识库时的标准输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendRetrieveRequest {
    pub question: String,
    pub top_k: usize,
    pub trace_id: Option<String>,
    pub session_id: Option<String>,
}

// 后端知识库检索条目：桥接到 RetrievedContext 前的统一中间格式。
#[derive(Debug, Clone, PartialEq)]
pub struct BackendRetrievedItem {
    pub alias: String,
    pub snippet: String,
    pub score: f64,
    pub source: Option<String>,
}

// 后端知识库端口：抽象远端向量库/检索服务。
pub trait KnowledgeBackend {
    fn retrieve(
        &self,
        request: &BackendRetrieveRequest,
    ) -> Result<Vec<BackendRetrievedItem>, DomainError>;
}

// Agent 问答请求：将问题和上下文一并交给上层 Agent/LLM 编排服务。
#[derive(Debug, Clone, PartialEq)]
pub struct AgentAnswerRequest {
    pub question: String,
    pub contexts: Vec<RetrievedContext>,
    pub trace_id: Option<String>,
    pub session_id: Option<String>,
}

// Agent 问答响应：当前 CLI 只消费元信息，answer_text 预留给后续输出扩展。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentAnswerResponse {
    pub engine_status: String,
    pub provider: Option<String>,
    pub trace_id: Option<String>,
    pub answer_text: Option<String>,
}

// Agent 问答端口：抽象上游 Agent 服务/模型编排层。
pub trait AgentBackend {
    fn answer(&self, request: &AgentAnswerRequest) -> Result<AgentAnswerResponse, DomainError>;
}

// 后端检索桥接器：把 KnowledgeBackend 适配为现有 Retriever 端口。
#[derive(Debug, Clone)]
pub struct BackendRetrieverBridge<B> {
    backend: B,
    trace_id: Option<String>,
    session_id: Option<String>,
}

impl<B> BackendRetrieverBridge<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            trace_id: None,
            session_id: None,
        }
    }

    pub fn with_trace(mut self, trace_id: Option<String>, session_id: Option<String>) -> Self {
        self.trace_id = trace_id;
        self.session_id = session_id;
        self
    }
}

impl<B: KnowledgeBackend> Retriever for BackendRetrieverBridge<B> {
    fn retrieve(
        &self,
        question: &str,
        top: usize,
        _snapshot: &IndexSnapshot,
    ) -> Result<Vec<RetrievedContext>, DomainError> {
        let request = BackendRetrieveRequest {
            question: question.to_string(),
            top_k: top,
            trace_id: self.trace_id.clone(),
            session_id: self.session_id.clone(),
        };

        let items = self.backend.retrieve(&request)?;
        Ok(items
            .into_iter()
            .map(|item| RetrievedContext {
                alias: item.alias,
                snippet: item.snippet,
                score: item.score,
                source_layer: item.source,
                record_id: None,
                chunk_id: None,
            })
            .collect())
    }
}

// Agent 应答桥接器：把 AgentBackend 适配为现有 AnswerEngine 端口。
#[derive(Debug, Clone)]
pub struct BackendAnswerEngineBridge<A> {
    backend: A,
    trace_id: Option<String>,
    session_id: Option<String>,
}

impl<A> BackendAnswerEngineBridge<A> {
    pub fn new(backend: A) -> Self {
        Self {
            backend,
            trace_id: None,
            session_id: None,
        }
    }

    pub fn with_trace(mut self, trace_id: Option<String>, session_id: Option<String>) -> Self {
        self.trace_id = trace_id;
        self.session_id = session_id;
        self
    }
}

impl<A: AgentBackend> AnswerEngine for BackendAnswerEngineBridge<A> {
    fn meta(
        &self,
        question: &str,
        contexts: &[RetrievedContext],
    ) -> Result<AnswerMeta, DomainError> {
        let request = AgentAnswerRequest {
            question: question.to_string(),
            contexts: contexts.to_vec(),
            trace_id: self.trace_id.clone(),
            session_id: self.session_id.clone(),
        };
        let response = self.backend.answer(&request)?;
        Ok(AnswerMeta {
            engine_status: response.engine_status,
            provider: response.provider,
            trace_id: response.trace_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AgentAnswerRequest, AgentAnswerResponse, AgentBackend, AnswerEngine,
        BackendAnswerEngineBridge, BackendRetrieveRequest, BackendRetrievedItem,
        BackendRetrieverBridge, KnowledgeBackend, Retriever,
    };
    use crate::domain::{IndexMeta, IndexSnapshot};
    use crate::error::DomainError;

    #[derive(Debug, Clone)]
    struct FakeKnowledgeBackend;

    impl KnowledgeBackend for FakeKnowledgeBackend {
        fn retrieve(
            &self,
            request: &BackendRetrieveRequest,
        ) -> Result<Vec<BackendRetrievedItem>, DomainError> {
            Ok(vec![BackendRetrievedItem {
                alias: format!("remote/{}", request.top_k),
                snippet: request.question.clone(),
                score: 0.88,
                source: Some("fake-kb".to_string()),
            }])
        }
    }

    #[derive(Debug, Clone)]
    struct FakeAgentBackend;

    impl AgentBackend for FakeAgentBackend {
        fn answer(
            &self,
            request: &AgentAnswerRequest,
        ) -> Result<AgentAnswerResponse, DomainError> {
            Ok(AgentAnswerResponse {
                engine_status: "enabled".to_string(),
                provider: Some("fake-agent".to_string()),
                trace_id: request.trace_id.clone(),
                answer_text: Some("ok".to_string()),
            })
        }
    }

    #[test]
    fn backend_retriever_bridge_maps_items_to_contexts() {
        let bridge = BackendRetrieverBridge::new(FakeKnowledgeBackend)
            .with_trace(Some("trace-1".to_string()), Some("sess-1".to_string()));
        let snapshot = IndexSnapshot {
            records: vec![],
            errors: vec![],
            meta: IndexMeta {
                full: false,
                baseline_size: 0,
                tracked_size: 0,
            },
        };

        let out = bridge
            .retrieve("hello", 2, &snapshot)
            .expect("bridge retrieve should succeed");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].alias, "remote/2");
        assert_eq!(out[0].snippet, "hello");
    }

    #[test]
    fn backend_answer_bridge_maps_agent_response_to_meta() {
        let bridge = BackendAnswerEngineBridge::new(FakeAgentBackend)
            .with_trace(Some("trace-2".to_string()), Some("sess-2".to_string()));
        let contexts = vec![];

        let meta = bridge
            .meta("question", &contexts)
            .expect("bridge meta should succeed");
        assert_eq!(meta.engine_status, "enabled");
        assert_eq!(meta.provider.as_deref(), Some("fake-agent"));
        assert_eq!(meta.trace_id.as_deref(), Some("trace-2"));
    }
}
