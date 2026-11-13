#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 || ! -f "$1" ]]; then
  printf 'Usage: bash %s <linux-package.tar.gz>\n' "$0" >&2
  exit 2
fi

archive_path="$(cd -- "$(dirname -- "$1")" && pwd)/$(basename -- "$1")"
if [[ "$(basename -- "$archive_path")" =~ ^kb-cli-([0-9]+\.[0-9]+\.[0-9]+([.-][A-Za-z0-9.-]+)?)-linux- ]]; then
  expected_version="kb ${BASH_REMATCH[1]}"
  expected_tag="v${BASH_REMATCH[1]}"
else
  printf 'Cannot infer package version from archive: %s\n' "$archive_path" >&2
  exit 1
fi
if [[ -n "${RELEASE_TAG:-}" && "$RELEASE_TAG" != "$expected_tag" ]]; then
  printf 'Release tag %s does not match package %s\n' "$RELEASE_TAG" "$expected_version" >&2
  exit 1
fi

test_root="$(mktemp -d "${TMPDIR:-/tmp}/kb-install-acceptance.XXXXXX")"
trap 'rm -rf -- "$test_root"' EXIT

mkdir -p "$test_root/package"
tar -xzf "$archive_path" -C "$test_root/package"
package_dir="$(find "$test_root/package" -mindepth 1 -maxdepth 1 -type d -print -quit)"
[[ -n "$package_dir" && -f "$package_dir/kb" && -f "$package_dir/install.sh" && -f "$package_dir/uninstall.sh" && -f "$package_dir/INSTALL.md" ]]
[[ -x "$package_dir/kb" && -x "$package_dir/install.sh" && -x "$package_dir/uninstall.sh" ]]

export HOME="$test_root/home"
export KB_GLOBAL_CONFIG="$test_root/global-config.json"
export KB_LLM_DISABLE_KEYRING=1
export KB_AGENT_ID=release-acceptance
export KB_SESSION_ID=package-smoke
export KB_INSTALL_DIR="$test_root/bin"
export KB_BIN_NAME=kb
mkdir -p "$HOME" "$KB_INSTALL_DIR"
export PATH="$KB_INSTALL_DIR:$PATH"

bash "$package_dir/install.sh"
[[ "$(kb --version)" == "$expected_version" ]]
kb --help >/dev/null

kb_root="$test_root/kb"
mkdir -p "$kb_root"
cd "$kb_root"
kb init --output json >/dev/null
printf '# Release package smoke\n\nThe packaged kb binary can index this local note.\n' > guide.md
kb add guide.md guide --output json >/dev/null
kb update --output json >/dev/null
kb ask "What can the packaged binary index?" --output json | grep -Fq 'local note'

printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"release-smoke","version":"1"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  '{"jsonrpc":"2.0","id":3,"method":"ping","params":{}}' \
  | kb serve --mcp \
  | python3 -c 'import json,sys; responses=[json.loads(line) for line in sys.stdin]; assert len(responses)==3, responses; assert responses[0]["result"]["protocolVersion"]=="2024-11-05", responses[0]; assert len(responses[1]["result"]["tools"])==8, responses[1]; assert responses[2]["id"]==3 and "error" not in responses[2], responses[2]'

printf 'preserve this unrelated file\n' > "$KB_INSTALL_DIR/keep.txt"
config_before="$(sha256sum "$KB_GLOBAL_CONFIG")"
bash "$package_dir/uninstall.sh"
[[ ! -e "$KB_INSTALL_DIR/kb" ]]
[[ -f "$KB_INSTALL_DIR/keep.txt" ]]
[[ "$(sha256sum "$KB_GLOBAL_CONFIG")" == "$config_before" ]]

printf 'Linux package install, CLI workflow, MCP, and uninstall passed.\n'
