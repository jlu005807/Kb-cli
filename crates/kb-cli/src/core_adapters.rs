use std::path::PathBuf;

use kb_core::{AnswerEngine, AnswerMeta, RetrievedContext, StateRef, StateSnapshot, StateStore};

use crate::state::{load_state, save_state, RefEntry, State};

pub struct FsStateStore {
    cwd: PathBuf,
}

impl FsStateStore {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }
}

impl StateStore for FsStateStore {
    fn load_state(&self) -> Result<StateSnapshot, String> {
        let cli_state = load_state(&self.cwd).map_err(|err| err.message.to_string())?;
        Ok(StateSnapshot {
            refs: cli_state
                .refs
                .into_iter()
                .map(|r| StateRef {
                    alias: r.alias,
                    path: r.path,
                    category: r.category,
                    perms: r.perms,
                })
                .collect(),
        })
    }

    fn save_state(&self, state: &StateSnapshot) -> Result<(), String> {
        let cli_state = State {
            refs: state
                .refs
                .iter()
                .map(|r| RefEntry {
                    alias: r.alias.clone(),
                    path: r.path.clone(),
                    category: r.category.clone(),
                    perms: r.perms.clone(),
                })
                .collect(),
        };
        save_state(&self.cwd, &cli_state).map_err(|err| err.message.to_string())
    }
}

pub struct DisabledAnswerEngine;

impl AnswerEngine for DisabledAnswerEngine {
    fn meta(&self, _question: &str, _contexts: &[RetrievedContext]) -> Result<AnswerMeta, String> {
        Ok(AnswerMeta {
            engine_status: "disabled".to_string(),
            provider: None,
            trace_id: None,
        })
    }
}

pub struct ConfiguredAnswerEngine {
    provider: String,
}

impl ConfiguredAnswerEngine {
    pub fn new(provider: String) -> Self {
        Self { provider }
    }
}

impl AnswerEngine for ConfiguredAnswerEngine {
    fn meta(&self, _question: &str, _contexts: &[RetrievedContext]) -> Result<AnswerMeta, String> {
        Ok(AnswerMeta {
            engine_status: "configured".to_string(),
            provider: Some(self.provider.clone()),
            trace_id: None,
        })
    }
}
