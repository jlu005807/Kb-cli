include!("core/shared.rs");
include!("core/guards.rs");
include!("core/registry_models.rs");
include!("core/resolve.rs");
include!("core/diff.rs");
include!("core/kbmd_ops.rs");

// Handlers split by command family (#1 optimization pass 2026-05).
// Each file owns one command surface and stays well under 600 lines; the
// `ask.rs` agent loop is still the heaviest at ~2.3k and remains a follow-up
// target for further sub-division.
include!("core/handlers/completion.rs");
include!("core/handlers/init.rs");
include!("core/handlers/env.rs");
include!("core/handlers/refs.rs");
include!("core/handlers/list.rs");
include!("core/handlers/check.rs");
include!("core/handlers/layer_issues.rs");
include!("core/handlers/update.rs");
include!("core/handlers/ask.rs");
