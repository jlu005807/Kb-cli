//! Usecase 编排层。
//! 负责把领域规则组织成可执行业务流程，并保持输入/输出稳定契约。

use crate::domain::Alias;
use crate::domain::IndexSnapshot;
use crate::ports::{AnswerEngine, AnswerMeta, RetrievedContext, Retriever, StateSnapshot};

// init 规划：只做“是否允许初始化/覆盖”的业务判定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitPlan {
    pub project: String,
    pub overwritten: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitPlanError {
    AlreadyInitialized,
}

pub fn plan_init(
    project_name_from_cwd: String,
    provided_name: Option<String>,
    already_initialized: bool,
    force: bool,
) -> Result<InitPlan, InitPlanError> {
    if already_initialized && !force {
        return Err(InitPlanError::AlreadyInitialized);
    }

    Ok(InitPlan {
        project: provided_name.unwrap_or(project_name_from_cwd),
        overwritten: force && already_initialized,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddModePlan {
    Link,
    Copy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddPlanInput {
    pub normalized_path: String,
    pub alias: Option<String>,
    pub mode: AddModePlan,
    pub requested_category: Option<String>,
    pub available_categories: Vec<String>,
    pub existing_aliases: Vec<String>,
    pub existing_paths: Vec<String>,
    pub ignored: bool,
    pub source_exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddPlan {
    pub alias: String,
    pub category: String,
    pub mode: AddModePlan,
    pub ignored: bool,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddPlanError {
    SourcePathNotFound {
        path: String,
    },
    InvalidAlias {
        alias: String,
    },
    AliasConflict {
        alias: String,
    },
    PathAlreadyRegistered {
        path: String,
    },
    CategoryNotFound {
        category: String,
        available: Vec<String>,
    },
}

// add 规划：聚合输入校验（路径、别名、分类、冲突）并给出标准化执行参数。
pub fn plan_add(input: AddPlanInput) -> Result<AddPlan, AddPlanError> {
    if !input.source_exists {
        return Err(AddPlanError::SourcePathNotFound {
            path: input.normalized_path,
        });
    }

    let alias = input.alias.unwrap_or_else(|| input.normalized_path.clone());

    if input.ignored {
        return Ok(AddPlan {
            alias,
            category: input
                .requested_category
                .or_else(|| input.available_categories.last().cloned())
                .unwrap_or_else(|| "raw".to_string()),
            mode: input.mode,
            ignored: true,
            path: input.normalized_path,
        });
    }

    if Alias::parse(&alias).is_err() {
        return Err(AddPlanError::InvalidAlias { alias });
    }

    if input.existing_aliases.iter().any(|x| x == &alias) {
        return Err(AddPlanError::AliasConflict { alias });
    }

    if input
        .existing_paths
        .iter()
        .any(|x| x == &input.normalized_path)
    {
        return Err(AddPlanError::PathAlreadyRegistered {
            path: input.normalized_path,
        });
    }

    let mut categories = input.available_categories;
    if categories.is_empty() {
        categories.push("raw".to_string());
    }

    let category = match input.requested_category {
        Some(requested) => {
            if !categories.iter().any(|x| x == &requested) {
                return Err(AddPlanError::CategoryNotFound {
                    category: requested,
                    available: categories,
                });
            }
            requested
        }
        None => categories
            .last()
            .cloned()
            .unwrap_or_else(|| "raw".to_string()),
    };

    Ok(AddPlan {
        alias,
        category,
        mode: input.mode,
        ignored: false,
        path: input.normalized_path,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MovePlanError {
    InvalidAlias { alias: String },
    AliasConflict { alias: String },
    NotFoundAlias { alias: String },
}

// mv 只改别名，不改 path/category/perms。
pub fn apply_mv(
    state: &StateSnapshot,
    alias: &str,
    new_alias: &str,
) -> Result<StateSnapshot, MovePlanError> {
    if Alias::parse(new_alias).is_err() {
        return Err(MovePlanError::InvalidAlias {
            alias: new_alias.to_string(),
        });
    }
    if state.refs.iter().any(|r| r.alias == new_alias) {
        return Err(MovePlanError::AliasConflict {
            alias: new_alias.to_string(),
        });
    }

    let mut next = state.clone();
    let Some(item) = next.refs.iter_mut().find(|r| r.alias == alias) else {
        return Err(MovePlanError::NotFoundAlias {
            alias: alias.to_string(),
        });
    };
    item.alias = new_alias.to_string();
    Ok(next)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemovePlanError {
    NotFoundAlias { alias: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovePlan {
    pub state: StateSnapshot,
    pub removed_alias: String,
    pub removed_path: String,
}

// rm 只移除引用关系，不触碰物理文件。
pub fn apply_rm(state: &StateSnapshot, alias: &str) -> Result<RemovePlan, RemovePlanError> {
    let mut next = state.clone();
    let Some(idx) = next.refs.iter().position(|r| r.alias == alias) else {
        return Err(RemovePlanError::NotFoundAlias {
            alias: alias.to_string(),
        });
    };
    let removed = next.refs.remove(idx);
    Ok(RemovePlan {
        state: next,
        removed_alias: removed.alias,
        removed_path: removed.path,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathPlanError {
    NotFoundAlias { alias: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathPlan {
    pub state: Option<StateSnapshot>,
    pub alias: String,
    pub path: String,
    pub old_path: Option<String>,
}

// path 支持“查询”和“更新”两种模式：new_path=None 为查询。
pub fn apply_path(
    state: &StateSnapshot,
    alias: &str,
    new_path: Option<String>,
) -> Result<PathPlan, PathPlanError> {
    let Some(current) = state.refs.iter().find(|r| r.alias == alias) else {
        return Err(PathPlanError::NotFoundAlias {
            alias: alias.to_string(),
        });
    };

    if let Some(new_path_value) = new_path {
        let mut next = state.clone();
        let item = next
            .refs
            .iter_mut()
            .find(|r| r.alias == alias)
            .expect("alias existence already checked");
        let old_path = item.path.clone();
        item.path = new_path_value.clone();
        Ok(PathPlan {
            state: Some(next),
            alias: alias.to_string(),
            path: new_path_value,
            old_path: Some(old_path),
        })
    } else {
        Ok(PathPlan {
            state: None,
            alias: alias.to_string(),
            path: current.path.clone(),
            old_path: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChmodPlanError {
    InvalidPermission { provided: String },
    NotFoundAlias { alias: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChmodPlan {
    pub state: StateSnapshot,
    pub alias: String,
    pub permissions: String,
}

// chmod 在 core 层统一校验权限串合法性，CLI 仅做错误映射。
pub fn apply_chmod(
    state: &StateSnapshot,
    alias: &str,
    perms: &str,
) -> Result<ChmodPlan, ChmodPlanError> {
    if crate::domain::Permission::parse(perms).is_err() {
        return Err(ChmodPlanError::InvalidPermission {
            provided: perms.to_string(),
        });
    }

    let mut next = state.clone();
    let Some(item) = next.refs.iter_mut().find(|r| r.alias == alias) else {
        return Err(ChmodPlanError::NotFoundAlias {
            alias: alias.to_string(),
        });
    };
    item.perms = perms.to_string();
    Ok(ChmodPlan {
        state: next,
        alias: alias.to_string(),
        permissions: perms.to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CategoryFilterError {
    CategoryNotFound { category: String },
}

pub fn validate_category_filter(
    requested: Option<&str>,
    categories: &[String],
) -> Result<(), CategoryFilterError> {
    if let Some(cat) = requested {
        if !categories.iter().any(|x| x == cat) {
            return Err(CategoryFilterError::CategoryNotFound {
                category: cat.to_string(),
            });
        }
    }
    Ok(())
}

// top 参数归一化：统一由 usecase 层决定默认值策略。
pub fn normalize_top(top: Option<u32>, default: u32) -> usize {
    top.unwrap_or(default) as usize
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRow {
    pub alias: String,
    pub path: String,
    pub perms: String,
    pub category: String,
    pub size: Option<u64>,
    pub mtime: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListPlan {
    pub items: Vec<ListRow>,
    pub categories: Option<Vec<String>>,
}

// list 输出规划：tree 模式附带分类目录，普通模式仅返回 items。
pub fn plan_list_output(items: Vec<ListRow>, tree: bool, categories: Vec<String>) -> ListPlan {
    ListPlan {
        items,
        categories: if tree { Some(categories) } else { None },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AskPlanError {
    NoRelevantContext,
}

// ask 前置判定：先判断是否存在可检索上下文，再确定 top 限额。
pub fn plan_ask(has_context: bool, top: Option<u32>) -> Result<usize, AskPlanError> {
    if !has_context {
        return Err(AskPlanError::NoRelevantContext);
    }
    Ok(top.unwrap_or(1).max(1) as usize)
}

#[derive(Debug, Clone, PartialEq)]
pub struct AskUsecaseOutput {
    pub meta: AnswerMeta,
    pub retrieved_context: Vec<RetrievedContext>,
}

// ask 主流程编排：retrieve -> meta，任何一步失败都转成 NoRelevantContext。
pub fn execute_ask<R: Retriever + ?Sized, A: AnswerEngine + ?Sized>(
    retriever: &R,
    answer_engine: &A,
    snapshot: &IndexSnapshot,
    question: &str,
    top: Option<u32>,
) -> Result<AskUsecaseOutput, AskPlanError> {
    let has_context = snapshot.records.iter().any(|record| record.present);
    let limit = plan_ask(has_context, top)?;
    let retrieved_context = retriever
        .retrieve(question, limit, snapshot)
        .map_err(|_| AskPlanError::NoRelevantContext)?;
    if retrieved_context.is_empty() {
        return Err(AskPlanError::NoRelevantContext);
    }
    let meta = answer_engine
        .meta(question, &retrieved_context)
        .map_err(|_| AskPlanError::NoRelevantContext)?;
    Ok(AskUsecaseOutput {
        meta,
        retrieved_context,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        apply_mv, plan_add, plan_ask, plan_init, AddModePlan, AddPlanError, AddPlanInput,
        AskPlanError, InitPlanError, MovePlanError,
    };
    use crate::domain::{IndexMeta, IndexRecord, IndexSnapshot};
    use crate::ports::{
        AnswerEngine, AnswerMeta, RetrievedContext, Retriever, StateRef, StateSnapshot,
    };

    struct TestRetriever;

    impl Retriever for TestRetriever {
        fn retrieve(
            &self,
            _question: &str,
            top: usize,
            _snapshot: &IndexSnapshot,
        ) -> Result<Vec<RetrievedContext>, String> {
            Ok(vec![RetrievedContext {
                alias: "docs/config".to_string(),
                snippet: "Use kb config to change settings.".to_string(),
                score: 0.95,
            }]
            .into_iter()
            .take(top)
            .collect())
        }
    }

    struct DisabledAnswerEngine;

    impl AnswerEngine for DisabledAnswerEngine {
        fn meta(
            &self,
            _question: &str,
            _contexts: &[RetrievedContext],
        ) -> Result<AnswerMeta, String> {
            Ok(AnswerMeta {
                engine_status: "disabled".to_string(),
                provider: None,
                trace_id: None,
            })
        }
    }

    #[test]
    fn init_plan_blocks_non_force_overwrite() {
        let result = plan_init("demo".to_string(), None, true, false);
        assert_eq!(result, Err(InitPlanError::AlreadyInitialized));
    }

    #[test]
    fn add_plan_rejects_unknown_category() {
        let result = plan_add(AddPlanInput {
            normalized_path: "./docs/a.md".to_string(),
            alias: Some("docs/a".to_string()),
            mode: AddModePlan::Link,
            requested_category: Some("unknown".to_string()),
            available_categories: vec!["raw".to_string()],
            existing_aliases: vec![],
            existing_paths: vec![],
            ignored: false,
            source_exists: true,
        });

        assert!(matches!(result, Err(AddPlanError::CategoryNotFound { .. })));
    }

    #[test]
    fn mv_plan_rejects_conflict() {
        let state = StateSnapshot {
            refs: vec![
                StateRef {
                    alias: "a".to_string(),
                    path: "./a".to_string(),
                    category: "raw".to_string(),
                    perms: "rwx".to_string(),
                },
                StateRef {
                    alias: "b".to_string(),
                    path: "./b".to_string(),
                    category: "raw".to_string(),
                    perms: "rwx".to_string(),
                },
            ],
        };

        let result = apply_mv(&state, "a", "b");
        assert!(matches!(result, Err(MovePlanError::AliasConflict { .. })));
    }

    #[test]
    fn ask_plan_requires_context() {
        let result = plan_ask(false, Some(3));
        assert_eq!(result, Err(AskPlanError::NoRelevantContext));
    }

    #[test]
    fn ask_usecase_returns_disabled_meta_and_context() {
        let snapshot = IndexSnapshot {
            records: vec![IndexRecord {
                alias: "docs/config".to_string(),
                path: "./docs/config.md".to_string(),
                category: "raw".to_string(),
                signature: "f:1".to_string(),
                present: true,
                excerpt: Some("Use kb config to change settings.".to_string()),
            }],
            errors: vec![],
            meta: IndexMeta {
                full: false,
                baseline_size: 0,
                tracked_size: 1,
            },
        };

        let out = super::execute_ask(
            &TestRetriever,
            &DisabledAnswerEngine,
            &snapshot,
            "hello",
            Some(1),
        )
        .expect("ask should succeed");

        assert_eq!(out.meta.engine_status, "disabled");
        assert_eq!(out.retrieved_context.len(), 1);
        assert_eq!(out.retrieved_context[0].alias, "docs/config");
    }
}
