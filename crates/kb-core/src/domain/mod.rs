//! 领域层聚合导出：
//! - `alias`：别名规则
//! - `permission`：权限规则
//! - `reference`：引用实体
//! - `index`：索引快照与差异引擎

mod alias;
mod index;
mod permission;
mod reference;

pub use alias::{Alias, AliasError, AliasErrorKind};
pub use index::{
    CheckOutcome, CheckResult, IndexConfig, IndexEngine, IndexErrorRecord, IndexMeta, IndexRecord,
    IndexSnapshot, ListItem, ListView, UpdateOutcome,
};
pub use permission::{
    Permission, PermissionAction, PermissionExplainTrace, PermissionInterpreter,
    PermissionParseError, PermissionParseErrorKind,
};
pub use reference::{Ref, RefError};
