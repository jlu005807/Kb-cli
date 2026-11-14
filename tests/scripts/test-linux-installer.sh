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

for fixture in regular symlink no-match special; do
  fixture_root="$test_root/$fixture"
  mkdir -p "$fixture_root/home"
  profile="$fixture_root/home/.bashrc"
  real_profile="$fixture_root/real-rc"
  printf '# original shell settings\n' > "$real_profile"
  chmod 0640 "$real_profile"
  if [[ "$fixture" == symlink ]]; then
    ln -s "$real_profile" "$profile"
  else
    cp -p "$real_profile" "$profile"
  fi
  install_dir="$fixture_root/bin"
  if [[ "$fixture" == special ]]; then install_dir="$fixture_root/dir\$token space\\back\"quote\`tick/bin"; fi
  if [[ "$fixture" != no-match ]]; then
    env HOME="$fixture_root/home" KB_INSTALL_DIR="$install_dir" PATH=/usr/bin:/bin bash "$package_dir/install.sh" >/dev/null
    env HOME="$fixture_root/home" KB_INSTALL_DIR="$install_dir" PATH=/usr/bin:/bin bash "$package_dir/install.sh" >/dev/null
    [[ "$(grep -Fc 'export PATH=' "$profile")" -eq 1 ]]
    env HOME="$fixture_root/home" PATH=/usr/bin:/bin token=EXPANDED bash -c 'source "$1"; [[ "${PATH%%:*}" == "$2" ]]' check "$profile" "$install_dir"
  fi
  env HOME="$fixture_root/home" KB_INSTALL_DIR="$install_dir" bash "$package_dir/uninstall.sh" >/dev/null
  [[ "$(stat -Lc %a "$profile")" == 640 ]]
  [[ "$(cat "$profile")" == '# original shell settings' ]]
  if [[ "$fixture" == symlink ]]; then
    [[ -L "$profile" && "$(cat "$real_profile")" == '# original shell settings' ]]
  fi
  printf 'Shell profile fixture passed: %s\n' "$fixture"
done

python3 - "$package_dir/uninstall.sh" "$test_root" <<'PY'
import os
from pathlib import Path
import resource
import subprocess
import sys

root = Path(sys.argv[2]) / 'write-failure'
root.mkdir()
target = root / 'real-rc'
original = '# keep\n' * 1024 + 'export PATH="$HOME/.local/bin:$PATH"\n'
target.write_text(original)
target.chmod(0o640)
profile = root / '.bashrc'
profile.symlink_to(target)
def limit_writes():
    resource.setrlimit(resource.RLIMIT_FSIZE, (1024, 1024))
result = subprocess.run(['bash', sys.argv[1]], env={**os.environ, 'HOME': str(root),
    'KB_INSTALL_DIR': str(root / '.local/bin')}, preexec_fn=limit_writes, capture_output=True, timeout=10)
assert result.returncode != 0, 'The injected write limit must fail the uninstall'
assert profile.is_symlink() and target.read_text() == original
assert target.stat().st_mode & 0o777 == 0o640
assert not list(root.glob('.kb-path.*'))
print('Shell profile write-failure fixture passed.')
PY

for name in ${!KB_@}; do unset "$name"; done
export HOME="$test_root/home"
export KB_GLOBAL_CONFIG="$test_root/global-config.json"
export KB_LLM_DISABLE_KEYRING=1
export KB_AGENT_ID=release-acceptance
export KB_SESSION_ID=package-smoke
mkdir -p "$HOME"

bash "$package_dir/install.sh"
source "$HOME/.profile"
[[ "$(command -v kb)" == "$HOME/.local/bin/kb" ]]
profile_before="$(cat "$HOME/.profile")"
bash "$package_dir/install.sh"
[[ "$(cat "$HOME/.profile")" == "$profile_before" ]]
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

printf 'preserve this unrelated file\n' > "$HOME/.local/bin/keep.txt"
config_before="$(sha256sum "$KB_GLOBAL_CONFIG")"
bash "$package_dir/uninstall.sh"
[[ ! -e "$HOME/.local/bin/kb" ]]
[[ -f "$HOME/.local/bin/keep.txt" ]]
[[ "$(cat "$HOME/.profile")" != *'export PATH='* ]]
[[ "$(sha256sum "$KB_GLOBAL_CONFIG")" == "$config_before" ]]

printf 'Linux package install, CLI workflow, MCP, and uninstall passed.\n'
