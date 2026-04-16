#!/usr/bin/env bash
set -euo pipefail

TARGET=""
PROFILE="release"
DIST_DIR="dist"
SKIP_BUILD=0

usage() {
  cat <<'EOF'
Usage: bash ./scripts/package.sh [options]

Options:
  --target <triple>   Rust target triple (recommended on non-Linux hosts)
  --profile <name>    Cargo profile (default: release)
  --dist-dir <dir>    Output directory (default: dist)
  --skip-build        Skip cargo build and package existing binary
  -h, --help          Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      TARGET="${2:-}"
      shift 2
      ;;
    --profile)
      PROFILE="${2:-}"
      shift 2
      ;;
    --dist-dir)
      DIST_DIR="${2:-}"
      shift 2
      ;;
    --skip-build)
      SKIP_BUILD=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf '[package.sh] Unknown argument: %s\n' "$1" >&2
      exit 1
      ;;
  esac
done

log() {
  printf '[package.sh] %s\n' "$1"
}

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
WORKSPACE_TOML="$REPO_ROOT/Cargo.toml"
HOST_UNAME="$(uname -s | tr '[:upper:]' '[:lower:]')"

is_windows_compat_host() {
  [[ "$HOST_UNAME" == mingw* || "$HOST_UNAME" == msys* || "$HOST_UNAME" == cygwin* ]]
}

is_linux_target() {
  local triple="$1"
  [[ "$triple" == *-linux-* ]]
}

get_workspace_version() {
  awk '
    BEGIN { in_workspace_pkg = 0 }
    /^\[workspace\.package\]/ { in_workspace_pkg = 1; next }
    /^\[/ && in_workspace_pkg { in_workspace_pkg = 0 }
    in_workspace_pkg && $1 == "version" {
      gsub(/"/, "", $3)
      print $3
      exit
    }
  ' "$WORKSPACE_TOML"
}

get_arch_label() {
  local triple="$1"
  if [[ "$triple" == x86_64-* ]]; then
    printf 'x86_64'
    return
  fi
  if [[ "$triple" == aarch64-* ]]; then
    printf 'aarch64'
    return
  fi
  if [[ "$triple" == i686-* ]]; then
    printf 'x86'
    return
  fi

  local raw_arch
  raw_arch="$(uname -m)"
  case "$raw_arch" in
    x86_64|amd64)
      printf 'x86_64'
      ;;
    aarch64|arm64)
      printf 'aarch64'
      ;;
    i386|i686|x86)
      printf 'x86'
      ;;
    *)
      printf '%s' "$raw_arch"
      ;;
  esac
}

if [[ -n "$TARGET" ]] && ! is_linux_target "$TARGET"; then
  printf '[package.sh] This script only packages Linux targets. Received: %s\n' "$TARGET" >&2
  printf '[package.sh] Use scripts/package.ps1 for Windows artifacts.\n' >&2
  exit 1
fi

if [[ -z "$TARGET" ]] && is_windows_compat_host; then
  printf '[package.sh] Running under Git Bash/MSYS/Cygwin requires an explicit Linux target.\n' >&2
  printf '[package.sh] Example: bash ./scripts/package.sh --target x86_64-unknown-linux-gnu\n' >&2
  exit 1
fi

VERSION="$(get_workspace_version)"
if [[ -z "$VERSION" ]]; then
  VERSION="0.1.0"
fi
ARCH_LABEL="$(get_arch_label "$TARGET")"

if [[ -n "$TARGET" ]] && command -v rustup >/dev/null 2>&1; then
  if ! rustup target list --installed | grep -qx "$TARGET"; then
    printf '[package.sh] Missing Rust target: %s\n' "$TARGET" >&2
    printf '[package.sh] Install it with: rustup target add %s\n' "$TARGET" >&2
    exit 1
  fi
fi

if [[ "$SKIP_BUILD" -eq 0 ]]; then
  build_args=(build -p kb-cli --profile "$PROFILE")
  if [[ -n "$TARGET" ]]; then
    build_args+=(--target "$TARGET")
  fi
  log "Running build: cargo ${build_args[*]}"
  (
    cd "$REPO_ROOT"
    cargo "${build_args[@]}"
  )
fi

if [[ -n "$TARGET" ]]; then
  BINARY_PATH="$REPO_ROOT/target/$TARGET/$PROFILE/kb-cli"
else
  BINARY_PATH="$REPO_ROOT/target/$PROFILE/kb-cli"
fi

if [[ ! -f "$BINARY_PATH" ]]; then
  printf '[package.sh] Binary not found: %s\n' "$BINARY_PATH" >&2
  exit 1
fi

PACKAGE_NAME="kb-cli-$VERSION-linux-$ARCH_LABEL"
DIST_ROOT="$REPO_ROOT/$DIST_DIR"
STAGE_DIR="$DIST_ROOT/$PACKAGE_NAME"
ARCHIVE_PATH="$DIST_ROOT/$PACKAGE_NAME.tar.gz"

rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR"

cp "$BINARY_PATH" "$STAGE_DIR/kb"
chmod +x "$STAGE_DIR/kb"

cp "$REPO_ROOT/packaging/linux/install.sh" "$STAGE_DIR/install.sh"
cp "$REPO_ROOT/packaging/linux/uninstall.sh" "$STAGE_DIR/uninstall.sh"
chmod +x "$STAGE_DIR/install.sh" "$STAGE_DIR/uninstall.sh"

cat > "$STAGE_DIR/INSTALL.md" <<'EOF'
# KB-CLI Linux Installer

## Files
- kb
- install.sh
- uninstall.sh

## One-command install
bash ./install.sh

## Uninstall
bash ./uninstall.sh
EOF

mkdir -p "$DIST_ROOT"
tar -czf "$ARCHIVE_PATH" -C "$DIST_ROOT" "$PACKAGE_NAME"

log "Package ready:"
printf '  Stage: %s\n' "$STAGE_DIR"
printf '  Tar  : %s\n' "$ARCHIVE_PATH"
