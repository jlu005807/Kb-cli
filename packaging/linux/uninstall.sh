#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="${KB_INSTALL_DIR:-$HOME/.local/bin}"
BIN_NAME="${KB_BIN_NAME:-kb}"
TARGET_BIN="$INSTALL_DIR/$BIN_NAME"

log() {
  printf '[kb-cli] %s\n' "$1"
}

if [[ -f "$TARGET_BIN" ]]; then
  rm -f "$TARGET_BIN"
  log "Removed binary: $TARGET_BIN"
else
  log "Binary not found: $TARGET_BIN"
fi

PATH_LINE=''
if [[ "$INSTALL_DIR" == "$HOME/.local/bin" ]]; then
  PATH_LINE='export PATH="$HOME/.local/bin:$PATH"'
else
  ESCAPED_INSTALL_DIR="${INSTALL_DIR//\"/\\\"}"
  PATH_LINE="export PATH=\"$ESCAPED_INSTALL_DIR:\$PATH\""
fi

remove_path_line() {
  local profile_file="$1"
  if [[ ! -f "$profile_file" ]]; then
    return
  fi

  local tmp_file
  tmp_file="$(mktemp)"
  grep -Fvx "$PATH_LINE" "$profile_file" > "$tmp_file" || true
  mv "$tmp_file" "$profile_file"
  log "Cleaned PATH entry in: $profile_file"
}

for profile in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile"; do
  remove_path_line "$profile"
done

if [[ -d "$INSTALL_DIR" ]]; then
  if [[ -z "$(ls -A "$INSTALL_DIR")" ]]; then
    rmdir "$INSTALL_DIR"
    log "Removed empty directory: $INSTALL_DIR"
  fi
fi

printf '\nUninstall complete.\n'
printf 'Reopen terminal to refresh PATH.\n'
