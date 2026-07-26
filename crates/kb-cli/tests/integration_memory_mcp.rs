//! Integration tests for the agent-memory system (`kb remember` / `kb correct` /
//! `kb consolidate`) and the MCP stdio server (`kb serve --mcp`).
//!
//! Follows the same conventions as `integration_core_commands.rs`:
//! `assert_cmd::Command::cargo_bin("kb-cli")`, unique temp dirs, `--output json`.
//! The MCP end-to-end test spawns the real binary and speaks JSON-RPC 2.0 over
//! piped stdio (line-delimited), matching the transport used by Claude Desktop
//! / Cursor / Cline.

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn run_json(args: &[&str], current_dir: &Path) -> (bool, Value) {
    let assert = Command::cargo_bin("kb-cli")
        .expect("kb-cli binary should build")
        .current_dir(current_dir)
        .arg("--output")
        .arg("json")
        .args(args)
        .assert();
    let success = assert.get_output().status.success();
    let body: Value = serde_json::from_slice(&assert.get_output().stdout).expect("json stdout");
    (success, body)
}

fn run_json_with_env(args: &[&str], current_dir: &Path, envs: &[(&str, &str)]) -> (bool, Value) {
    let mut cmd = Command::cargo_bin("kb-cli").expect("kb-cli binary should build");
    cmd.current_dir(current_dir)
        .arg("--output")
        .arg("json")
        .args(args);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let assert = cmd.assert();
    let success = assert.get_output().status.success();
    let body: Value = serde_json::from_slice(&assert.get_output().stdout).expect("json stdout");
    (success, body)
}

fn unique_temp_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "kb_cli_mem_{}_{}_{}",
        tag,
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&dir).expect("temp dir should be created");
    dir
}

fn mk_file(root: &Path, rel: &str, content: &str) {
    let abs = root.join(rel.trim_start_matches("./"));
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent).expect("create parent dirs");
    }
    fs::write(abs, content).expect("write file");
}

/// Parse `.kb/episodes.jsonl` into JSON values, one per line (skipping blanks).
fn read_episodes(root: &Path) -> Vec<Value> {
    let raw =
        fs::read_to_string(root.join(".kb/episodes.jsonl")).expect("episodes.jsonl should exist");
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).expect("episode line should be valid JSON"))
        .collect()
}

/// Episode ids follow `ep-<unix nanos>-<16 hex chars>` (see ask/episodes.rs).
fn assert_episode_id_shape(id: &str) {
    let rest = id
        .strip_prefix("ep-")
        .unwrap_or_else(|| panic!("episode id should start with 'ep-': {id}"));
    let (nanos, hash) = rest
        .rsplit_once('-')
        .unwrap_or_else(|| panic!("episode id should have nanos and hash parts: {id}"));
    assert!(
        !nanos.is_empty() && nanos.chars().all(|c| c.is_ascii_digit()),
        "nanos part should be all digits: {id}"
    );
    assert_eq!(hash.len(), 16, "hash part should be 16 chars: {id}");
    assert!(
        hash.chars().all(|c| c.is_ascii_hexdigit()),
        "hash part should be lowercase hex: {id}"
    );
}

// ---------------------------------------------------------------------------
// kb remember / kb correct
// ---------------------------------------------------------------------------

#[test]
fn remember_writes_observation_episode_with_default_confidence() {
    let dir = unique_temp_dir("remember-observation");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    let (ok, body) = run_json_with_env(
        &["remember", "the storage backend defaults to fs"],
        &dir,
        &[("KB_AGENT_ID", "test-agent")],
    );
    assert!(ok);
    assert_eq!(body["status"], "success");
    assert_eq!(body["data"]["command"], "remember");
    assert_eq!(body["data"]["kind"], "observation");
    assert_eq!(body["data"]["agent_id"], "test-agent");
    let confidence = body["data"]["confidence"].as_f64().expect("confidence");
    assert!((confidence - 0.8).abs() < 1e-6, "default confidence is 0.8");

    let episode_id = body["data"]["episode_id"].as_str().expect("episode_id");
    assert_episode_id_shape(episode_id);

    let episodes = read_episodes(&dir);
    assert_eq!(episodes.len(), 1, "exactly one episode should be appended");
    let ep = &episodes[0];
    assert_eq!(ep["id"], episode_id);
    assert_eq!(ep["kind"], "observation");
    assert_eq!(ep["agent_id"], "test-agent");
    assert_eq!(ep["content"], "the storage backend defaults to fs");
    let stored_conf = ep["confidence"].as_f64().expect("stored confidence");
    assert!((stored_conf - 0.8).abs() < 1e-6);
}

#[test]
fn remember_supports_explicit_confidence_and_refs() {
    let dir = unique_temp_dir("remember-explicit");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    let (ok, body) = run_json(
        &[
            "remember",
            "chunk-scoped fact",
            "--ref",
            "docs/a:0001",
            "--confidence",
            "0.5",
        ],
        &dir,
    );
    assert!(ok);
    let confidence = body["data"]["confidence"].as_f64().expect("confidence");
    assert!((confidence - 0.5).abs() < 1e-6);
    assert_eq!(body["data"]["refs"][0], "docs/a:0001");
    // default agent id when KB_AGENT_ID is not set
    assert_eq!(body["data"]["agent_id"], "kb-cli");

    let episodes = read_episodes(&dir);
    assert_eq!(episodes[0]["refs"][0], "docs/a:0001");
}

#[test]
fn correct_writes_user_correction_with_confidence_1() {
    let dir = unique_temp_dir("correct-user-correction");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    let (ok, body) = run_json(&["correct", "the backend is fs, not sqlite"], &dir);
    assert!(ok);
    assert_eq!(body["status"], "success");
    assert_eq!(body["data"]["command"], "correct");
    assert_eq!(body["data"]["kind"], "user_correction");
    let episode_id = body["data"]["episode_id"].as_str().expect("episode_id");
    assert_episode_id_shape(episode_id);

    let episodes = read_episodes(&dir);
    assert_eq!(episodes.len(), 1);
    let ep = &episodes[0];
    assert_eq!(ep["kind"], "user_correction");
    let stored_conf = ep["confidence"].as_f64().expect("stored confidence");
    assert!(
        (stored_conf - 1.0).abs() < 1e-6,
        "user corrections are pinned to confidence 1.0"
    );
}

// ---------------------------------------------------------------------------
// kb consolidate
// ---------------------------------------------------------------------------

#[test]
fn consolidate_dry_run_plans_but_writes_nothing() {
    let dir = unique_temp_dir("consolidate-dryrun");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    let (ok, _) = run_json(&["remember", "fact one about the project"], &dir);
    assert!(ok);
    let (ok, _) = run_json(&["remember", "fact two about the project"], &dir);
    assert!(ok);

    let (ok, body) = run_json(&["consolidate", "--min", "1", "--dry-run"], &dir);
    assert!(ok);
    assert_eq!(body["data"]["dry_run"], true);
    assert_eq!(body["data"]["considered"], 2);
    assert!(body["data"]["memories_planned"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(
        body["data"]["written"].as_array().map(Vec::len),
        Some(0),
        "dry-run must not write memory files"
    );
    assert!(
        !dir.join(".kb/raw/agent-memory").exists(),
        "dry-run must not create the agent-memory dir"
    );
}

#[test]
fn consolidate_min_1_writes_memory_files_and_declares_them() {
    let dir = unique_temp_dir("consolidate-write");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    // No --ref: assertive episodes without refs are clustered under "general".
    let (ok, _) = run_json(&["remember", "workspace uses hexagonal architecture"], &dir);
    assert!(ok);
    let (ok, _) = run_json(&["correct", "the default backend is fs"], &dir);
    assert!(ok);

    let (ok, body) = run_json(&["consolidate", "--min", "1"], &dir);
    assert!(ok);
    assert_eq!(body["data"]["dry_run"], false);
    let written = body["data"]["written"].as_array().expect("written array");
    assert_eq!(written.len(), 1, "both episodes share the 'general' topic");
    assert_eq!(written[0]["topic"], "general");
    assert_eq!(written[0]["file"], "./.kb/raw/agent-memory/general.md");
    assert_eq!(written[0]["has_user_correction"], true);
    assert_eq!(written[0]["episodes"], 2);

    let memory_path = dir.join(".kb/raw/agent-memory/general.md");
    assert!(memory_path.is_file(), "memory file must exist on disk");
    let doc = fs::read_to_string(&memory_path).expect("read memory file");
    assert!(doc.contains("kb-agent-memory"), "provenance meta block");
    assert!(doc.contains("workspace uses hexagonal architecture"));
    assert!(doc.contains("the default backend is fs"));

    let added = body["data"]["added"].as_array().expect("added array");
    assert!(
        added
            .iter()
            .any(|x| x["alias"] == "agent-memory/general"
                && x["file"] == "./.kb/raw/agent-memory/general.md"),
        "memory file should be auto-declared via kb add"
    );

    // Re-running consolidate is idempotent at declaration level: the add
    // conflict is swallowed, the file is rewritten, and no error escapes.
    let (ok, again) = run_json(&["consolidate", "--min", "1"], &dir);
    assert!(ok);
    assert_eq!(again["status"], "success");
}

#[test]
fn consolidate_then_update_makes_memory_retrievable_via_ask() {
    let dir = unique_temp_dir("consolidate-retrieve");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);

    let (ok, _) = run_json(
        &["remember", "zebravault ingestion runs on port 4242"],
        &dir,
    );
    assert!(ok);
    let (ok, consolidated) = run_json(&["consolidate", "--min", "1"], &dir);
    assert!(ok);
    assert!(
        consolidated["data"]["written"]
            .as_array()
            .map(|w| !w.is_empty())
            .unwrap_or(false)
    );

    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);

    let (ok, ask) = run_json(&["ask", "zebravault", "--top", "3"], &dir);
    assert!(ok);
    assert_eq!(ask["data"]["mode"], "retrieve");
    let retrieved = ask["data"]["retrieved_context"]
        .as_array()
        .expect("retrieved_context array");
    assert!(
        !retrieved.is_empty(),
        "consolidated memory should be retrievable after kb update"
    );
    assert!(
        retrieved.iter().any(|item| {
            let from_memory = item["alias"]
                .as_str()
                .map(|a| a.contains("agent-memory"))
                .unwrap_or(false)
                || item["record_id"]
                    .as_str()
                    .map(|r| r.contains("agent-memory"))
                    .unwrap_or(false);
            let snippet_hit = item["snippet"]
                .as_str()
                .map(|s| s.contains("zebravault"))
                .unwrap_or(false);
            from_memory && snippet_hit
        }),
        "an agent-memory context containing the remembered fact should rank in: {retrieved:?}"
    );
}

// ---------------------------------------------------------------------------
// MCP server end-to-end over stdio (JSON-RPC 2.0, line-delimited)
// ---------------------------------------------------------------------------

const MCP_TOOLS: [&str; 8] = [
    "recall",
    "remember",
    "correct",
    "verify_citation",
    "list_episodes",
    "consolidate",
    "read_summary",
    "read_neighbors",
];

/// Extract the JSON payload of a `tools/call` response
/// (`result.content[0].text` holds pretty-printed JSON per the MCP tool contract).
fn tool_payload(resp: &Value) -> Value {
    assert_eq!(
        resp["result"]["isError"], false,
        "tool call should not be a business error: {resp}"
    );
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .expect("tool content text");
    serde_json::from_str(text).expect("tool content should be JSON")
}

#[test]
fn mcp_server_stdio_end_to_end() {
    let dir = unique_temp_dir("mcp-stdio-e2e");
    let (ok, _) = run_json(&["init", "demo"], &dir);
    assert!(ok);
    // Give `recall` something to hit: one indexed document.
    mk_file(
        &dir,
        "./docs/protocol.md",
        "The quargle handshake uses stdio JSON-RPC framing.\nSecond line.",
    );
    let (ok, _) = run_json(
        &["add", "./docs/protocol.md", "docs/protocol", "--category", "raw"],
        &dir,
    );
    assert!(ok);
    let (ok, _) = run_json(&["update"], &dir);
    assert!(ok);

    let bin = assert_cmd::cargo::cargo_bin("kb-cli");
    let mut child = std::process::Command::new(bin)
        .current_dir(&dir)
        .env("KB_AGENT_ID", "mcp-e2e-agent")
        .args(["serve", "--mcp"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn kb serve --mcp");

    {
        let stdin = child.stdin.as_mut().expect("child stdin");
        let requests = [
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"it","version":"0"}}}"#.to_string(),
            // Notification (no id): must be consumed without a response line.
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.to_string(),
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#.to_string(),
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"remember","arguments":{"content":"quargle framing is line-delimited"}}}"#.to_string(),
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"recall","arguments":{"query":"quargle handshake","top_k":3}}}"#.to_string(),
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"list_episodes","arguments":{"agent_id":"mcp-e2e-agent"}}}"#.to_string(),
        ];
        for req in &requests {
            writeln!(stdin, "{req}").expect("write request line");
        }
        stdin.flush().expect("flush stdin");
    }
    // Dropping stdin closes the pipe -> server sees EOF -> clean exit(0).
    drop(child.stdin.take());

    // Watchdog: a hung server must fail this test in seconds, not stall the
    // whole CI job until the runner-level timeout. Drain the pipes on helper
    // threads (so the child never blocks on a full pipe) and poll try_wait
    // with a hard deadline, killing the child on overrun.
    let mut stdout_pipe = child.stdout.take().expect("child stdout");
    let mut stderr_pipe = child.stderr.take().expect("child stderr");
    let stdout_thread = std::thread::spawn(move || {
        use std::io::Read as _;
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
        buf
    });
    let stderr_thread = std::thread::spawn(move || {
        use std::io::Read as _;
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf);
        buf
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let status = loop {
        match child.try_wait().expect("try_wait on mcp server") {
            Some(status) => break status,
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("kb serve --mcp did not exit within 60s after stdin EOF (killed)");
            }
            None => std::thread::sleep(std::time::Duration::from_millis(25)),
        }
    };
    let stdout_bytes = stdout_thread.join().expect("join stdout reader");
    let stderr_bytes = stderr_thread.join().expect("join stderr reader");
    assert!(
        status.success(),
        "server should exit 0 on stdin EOF; stderr: {}",
        String::from_utf8_lossy(&stderr_bytes)
    );

    let stdout = String::from_utf8(stdout_bytes).expect("utf8 stdout");
    let responses: Vec<Value> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).expect("response line should be JSON"))
        .collect();
    assert_eq!(
        responses.len(),
        5,
        "5 requests with ids -> 5 responses; the notification must not be answered. got: {stdout}"
    );
    for (idx, resp) in responses.iter().enumerate() {
        assert_eq!(resp["jsonrpc"], "2.0");
        assert_eq!(resp["id"], (idx as u64) + 1, "responses arrive in order");
    }

    // initialize
    let init = &responses[0];
    assert_eq!(init["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(init["result"]["serverInfo"]["name"], "kb-cli");
    assert!(init["result"]["capabilities"]["tools"].is_object());

    // tools/list: exactly the 8 advertised tools
    let tools = responses[1]["result"]["tools"]
        .as_array()
        .expect("tools array");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert_eq!(names.len(), 8, "exactly 8 tools: {names:?}");
    for expected in MCP_TOOLS {
        assert!(names.contains(&expected), "missing tool {expected}");
    }
    for tool in tools {
        assert!(tool["description"].is_string());
        assert!(tool["inputSchema"].is_object());
    }

    // tools/call remember: returns episode data
    let remembered = tool_payload(&responses[2]);
    assert_eq!(remembered["command"], "remember");
    assert_eq!(remembered["kind"], "observation");
    assert_eq!(remembered["agent_id"], "mcp-e2e-agent");
    assert_episode_id_shape(remembered["episode_id"].as_str().expect("episode_id"));

    // tools/call recall: ranked contexts from the updated index
    let recalled = tool_payload(&responses[3]);
    assert_eq!(recalled["query"], "quargle handshake");
    let results = recalled["results"].as_array().expect("results array");
    assert!(!results.is_empty(), "recall should hit the indexed doc");
    assert!(
        results.iter().any(|r| r["snippet"]
            .as_str()
            .map(|s| s.contains("quargle handshake"))
            .unwrap_or(false)),
        "recall snippet should contain the query phrase: {results:?}"
    );
    assert!(results[0]["score"].as_f64().unwrap_or(0.0) > 0.0);

    // tools/call list_episodes: includes the episode written via MCP remember
    let listed = tool_payload(&responses[4]);
    assert!(listed["count"].as_u64().unwrap_or(0) >= 1);
    let episodes = listed["episodes"].as_array().expect("episodes array");
    assert!(
        episodes.iter().any(|ep| ep["kind"] == "observation"
            && ep["content"] == "quargle framing is line-delimited"
            && ep["agent_id"] == "mcp-e2e-agent"),
        "list_episodes should return the remembered observation: {episodes:?}"
    );
}
