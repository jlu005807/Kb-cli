include!("core/shared.rs");
include!("core/guards.rs");
include!("core/registry_models.rs");
include!("core/resolve.rs");
include!("core/diff.rs");
include!("core/kbmd_ops.rs");

// Handlers split by command family (#1 optimization pass 2026-05).
// Each file owns one command surface and stays well under 600 lines; the
// `ask` entrypoint was further split into 4 sub-files (formatting / tools /
// http-traces / agent loop) during the 2026-05 follow-up to keep the heaviest
// agent surface manageable.
include!("core/handlers/completion.rs");
include!("core/handlers/init.rs");
include!("core/handlers/env.rs");
include!("core/handlers/refs.rs");
include!("core/handlers/list.rs");
include!("core/handlers/check.rs");
include!("core/handlers/layer_issues.rs");
include!("core/handlers/update.rs");
include!("core/handlers/ask/http.rs");
include!("core/handlers/ask/formatting.rs");
include!("core/handlers/ask/tools.rs");
include!("core/handlers/ask/episodes.rs");
include!("core/handlers/ask/agent.rs");
include!("core/handlers/memory.rs");
include!("core/handlers/mcp/protocol.rs");
include!("core/handlers/mcp/schema.rs");
include!("core/handlers/mcp/dispatch.rs");
include!("core/handlers/mcp/tools.rs");
