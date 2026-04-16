//! `kb-core` 是业务核心层：
//! - `domain`：领域模型与规则
//! - `usecases`：业务编排
//! - `ports`：对外依赖抽象（存储/检索/应答）
//! - `config`：配置合并与 trace
//!
//! 对外通过 `pub use` 暴露稳定 API，供 CLI 与适配层使用。

pub mod config;
pub mod domain;
pub mod ports;
pub mod usecases;

pub use config::{ConfigLayers, ConfigMergeTrace, ConfigSource, MergeResult};
pub use domain::{
    Alias, AliasError, AliasErrorKind, Permission, PermissionAction, PermissionExplainTrace,
    PermissionInterpreter, PermissionParseError, PermissionParseErrorKind, Ref, RefError,
};
pub use ports::{
    AgentAnswerRequest, AgentAnswerResponse, AgentBackend, AnswerEngine, AnswerMeta,
    BackendAnswerEngineBridge, BackendRetrieveRequest, BackendRetrievedItem,
    BackendRetrieverBridge, IndexStore, KnowledgeBackend, RetrievedContext, Retriever, StateRef,
    StateSnapshot, StateStore,
};
