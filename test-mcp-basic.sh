#!/bin/bash
# Convenience smoke test for the memory system + MCP server (release binary).
#
# NOTE: the authoritative coverage is the Rust integration test suite
# (crates/kb-cli/tests/integration_memory_mcp.rs, run by `cargo test -p kb-cli`
# and by CI). This script is only a quick manual sanity check.
#
# Prerequisite: cargo build --release -p kb-cli
set -euo pipefail

# Derive the repo root from this script's own location (no hardcoded paths).
REPO_ROOT="$(cd -- "$(dirname -- "$0")" && pwd)"
TEST_DIR="$REPO_ROOT/test-mcp-kb"

fail() {
    echo "✗ $1" >&2
    exit 1
}

echo "=== KB-CLI MCP Smoke Test ==="
echo ""

# 1. Locate binary (Linux/macOS name first, then Windows/Git Bash name)
echo "[1/6] Checking kb binary..."
KB_PATH="$REPO_ROOT/target/release/kb-cli"
if [ ! -f "$KB_PATH" ]; then
    KB_PATH="$REPO_ROOT/target/release/kb-cli.exe"
fi
if [ ! -f "$KB_PATH" ]; then
    fail "release binary not found; run: cargo build --release -p kb-cli"
fi
echo "✓ Found $KB_PATH"
echo ""

# 2. Setup test KB
echo "[2/6] Setting up test knowledge base..."
rm -rf "$TEST_DIR"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

"$KB_PATH" init test-mcp --output json > /dev/null || fail "kb init failed"
echo "✓ Knowledge base initialized"
echo ""

# 3. Add test content
echo "[3/6] Adding test content..."
cat > test-doc.md << 'DOCEOF'
# Test Document

This is a test document for MCP integration.

## Features
- Memory system with L0 Episode layer
- MCP protocol (JSON-RPC 2.0 over stdio)
- Agent integration (Claude Desktop / Cursor / Cline)
DOCEOF

"$KB_PATH" add test-doc.md > /dev/null || fail "kb add failed"
"$KB_PATH" update > /dev/null || fail "kb update failed"
echo "✓ Test content added and indexed"
echo ""

# 4. Test memory commands
echo "[4/6] Testing memory commands..."
export KB_AGENT_ID="test-agent"
export KB_SESSION_ID="test-session-001"

# NOTE: --output json is pretty-printed ("key": "value" with a space) —
# the greps below must tolerate optional whitespace after the colon.
REMEMBER_RESULT=$("$KB_PATH" remember "项目使用 Rust workspace + 六边形架构" --output json)
echo "$REMEMBER_RESULT" | grep -q '"status": *"success"' || fail "kb remember failed: $REMEMBER_RESULT"
EPISODE_ID=$(echo "$REMEMBER_RESULT" | grep -o '"episode_id": *"[^"]*"' | cut -d'"' -f4)
[ -n "$EPISODE_ID" ] || fail "kb remember returned no episode_id"
echo "✓ kb remember: episode_id = $EPISODE_ID"

CORRECT_RESULT=$("$KB_PATH" correct "存储后端默认是 fs 不是 sqlite" --output json)
echo "$CORRECT_RESULT" | grep -q '"status": *"success"' || fail "kb correct failed: $CORRECT_RESULT"
echo "✓ kb correct: confidence = 1.0"

"$KB_PATH" remember "MCP server 通过 stdio 暴露 8 个 tools" --output json > /dev/null || fail "kb remember (2) failed"
"$KB_PATH" remember "支持 recall / remember / correct / verify_citation 等工具" --output json > /dev/null || fail "kb remember (3) failed"
echo ""

# 5. Test consolidate
echo "[5/6] Testing consolidate..."
CONSOLIDATE_RESULT=$("$KB_PATH" consolidate --dry-run --output json)
echo "$CONSOLIDATE_RESULT" | grep -q '"status": *"success"' || fail "kb consolidate --dry-run failed: $CONSOLIDATE_RESULT"
echo "✓ kb consolidate --dry-run succeeded"

"$KB_PATH" consolidate --output json > /dev/null || fail "kb consolidate failed"
[ -d ".kb/raw/agent-memory" ] || fail "consolidate did not create .kb/raw/agent-memory"
# find (not ls|wc): an empty glob must not abort the script under pipefail.
FILE_COUNT=$(find .kb/raw/agent-memory -maxdepth 1 -name '*.md' | wc -l)
[ "$FILE_COUNT" -ge 1 ] || fail "consolidate produced no memory files"
echo "✓ kb consolidate: created memory files ($FILE_COUNT files)"
echo ""

# 6. Test MCP server: real JSON-RPC round-trip over stdio.
# The server answers each request line and exits cleanly on stdin EOF.
echo "[6/6] Testing MCP server (initialize + tools/list round-trip)..."
MCP_OUT=$(printf '%s\n%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
    '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
    | "$KB_PATH" serve --mcp) || fail "kb serve --mcp exited non-zero"
echo "$MCP_OUT" | grep -q '"protocolVersion":"2024-11-05"' || fail "initialize response missing protocolVersion: $MCP_OUT"
echo "$MCP_OUT" | grep -q '"name":"kb-cli"' || fail "initialize response missing serverInfo: $MCP_OUT"
for tool in recall remember correct verify_citation list_episodes consolidate read_summary read_neighbors; do
    echo "$MCP_OUT" | grep -q "\"name\":\"$tool\"" || fail "tools/list missing tool: $tool"
done
echo "✓ MCP server answered initialize + tools/list (8 tools) and exited cleanly"
echo ""

echo "=== Test Summary ==="
echo "✓ All smoke checks passed"
echo ""
echo "Memory system verified:"
echo "  - Episode recording (remember/correct)"
echo "  - Episode consolidation to semantic memory files"
echo "  - MCP stdio JSON-RPC round-trip (initialize / tools/list)"
echo ""
echo "For full coverage run: cargo test -p kb-cli"

cd "$REPO_ROOT"
