use std::path::Path;

pub fn resolve_llm_config(cwd: &Path) -> kb_llm::LlmRuntimeConfig {
    let global_config_path = crate::env_registry::resolve_global_config_path();
    kb_llm::resolve_llm_config(cwd, &global_config_path)
}
