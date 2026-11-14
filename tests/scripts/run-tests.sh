#!/usr/bin/env bash
set -euo pipefail

echo "Running cargo check --workspace"
cargo check --workspace

echo "Running cargo test --workspace"
cargo test --workspace

echo "Running Node-based doc-fixture diff (fallback)"
if command -v node >/dev/null 2>&1; then
  node --test tests/tools/packaging.test.js
  node --test tests/tools/validate-doc-fixture-diff.test.js tests/tools/release-workflow.test.js
  node tests/tools/validate-doc-fixture-diff.js
else
  echo "node not found; skipping Node-based diff"
fi

echo "All tests completed."
