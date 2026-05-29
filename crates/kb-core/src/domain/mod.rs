//! 领域层聚合导出：
//! - `alias`：别名规则
//! - `permission`：权限规则
//! - `reference`：引用实体
//! - `index`：索引快照与差异引擎

mod agent;
mod alias;
mod episode;
mod graph;
mod index;
mod knowledge;
mod permission;
mod reference;
mod search;
mod source;
mod timestamp;
mod wiki_view;

pub use agent::{AgentId, AgentIdError};
pub use alias::{Alias, AliasError, AliasErrorKind};
pub use episode::{Episode, EpisodeFilter, EpisodeKind};
pub use graph::{GraphEdge, GraphEdgeType, GraphEvidence, GraphNode, GraphNodeType, GraphSnapshot};
pub use index::{
    CheckOutcome, CheckResult, IndexConfig, IndexEngine, IndexErrorRecord, IndexMeta, IndexRecord,
    IndexSnapshot, ListItem, ListView, UpdateOutcome,
};
pub use knowledge::{
    KnowledgeChunk, KnowledgeEntry, KnowledgeSnapshot, KnowledgeStatus, SemanticProviderInfo,
};
pub use source::KnowledgeSource;
pub use timestamp::Timestamp;
pub use permission::{
    Permission, PermissionAction, PermissionExplainTrace, PermissionInterpreter,
    PermissionParseError, PermissionParseErrorKind,
};
pub use reference::{Ref, RefError};
pub use search::{SearchHit, SearchProjectionSnapshot};
pub use wiki_view::{
    WikiManifestEntry, WikiManifestSnapshot, WikiPageDocument, WikiPageType, WikiViewSnapshot,
};
