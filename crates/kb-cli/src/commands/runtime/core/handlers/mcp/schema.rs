// `include!` 片段——见 ask/formatting.rs 顶部说明。
//
// Phase 3：MCP tools 的 schema 声明（tools/list 返回的内容）。
//
// 每个工具对应一个已有的 use case，只做协议翻译。inputSchema 用 JSON Schema
// 描述参数，MCP host 会据此把工具描述塞进 LLM 的 system prompt。

/// 返回全部 MCP 工具的 schema 列表。
fn mcp_tools_schema() -> Vec<Value> {
    vec![
        json!({
            "name": "recall",
            "description": "Search the project knowledge base and return ranked retrieved contexts. Use this before answering to ground responses in stored memory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What to search for." },
                    "top_k": { "type": "integer", "description": "Max results (default 5)." }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "remember",
            "description": "Record an agent observation into the L0 episode timeline. Use this to persist a fact learned during the session so future sessions can recall it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "content": { "type": "string", "description": "The fact / observation to remember." },
                    "refs": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "chunk_id(s) this observation references."
                    },
                    "confidence": { "type": "number", "description": "Self-assessed confidence 0.0-1.0 (default 0.8)." }
                },
                "required": ["content"]
            }
        }),
        json!({
            "name": "correct",
            "description": "Record a user correction (highest-priority memory signal, confidence 1.0). Use this when the user says a previous belief was wrong.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "content": { "type": "string", "description": "The corrected statement." },
                    "refs": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "chunk_id(s) this correction applies to."
                    }
                },
                "required": ["content"]
            }
        }),
        json!({
            "name": "verify_citation",
            "description": "Verify that a quote actually appears in a cited chunk. Lexical check (substring / token-overlap). Use this to confirm a citation before trusting it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "chunk_id": { "type": "string", "description": "The chunk_id the quote is attributed to." },
                    "quote": { "type": "string", "description": "The text claimed to be in that chunk." }
                },
                "required": ["chunk_id", "quote"]
            }
        }),
        json!({
            "name": "list_episodes",
            "description": "List recent episodes from the L0 timeline, optionally filtered by agent_id. Use this to audit what has been recorded.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "Filter to one agent's episodes." },
                    "limit": { "type": "integer", "description": "Max episodes to return (most recent, default 20)." }
                }
            }
        }),
        json!({
            "name": "consolidate",
            "description": "Distill accumulated episodes into agent-memory files. Pass dry_run=true to preview the plan without writing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "min": { "type": "integer", "description": "Min episodes per topic before a memory is produced (default 2)." },
                    "dry_run": { "type": "boolean", "description": "Preview only; do not write files." }
                }
            }
        }),
        json!({
            "name": "read_summary",
            "description": "Read the semantic summary, tags, and concepts for a record_id or path.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "record_id_or_path": { "type": "string" }
                },
                "required": ["record_id_or_path"]
            }
        }),
        json!({
            "name": "read_neighbors",
            "description": "Read knowledge-graph neighbors for a record_id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "record_id": { "type": "string" }
                },
                "required": ["record_id"]
            }
        }),
    ]
}
