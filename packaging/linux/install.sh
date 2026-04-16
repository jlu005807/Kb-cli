#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="${KB_INSTALL_DIR:-$HOME/.local/bin}"
BIN_NAME="${KB_BIN_NAME:-kb}"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

log() {
  printf '[kb-cli] %s\n' "$1"
}

SOURCE_BIN=""
for candidate in "$SCRIPT_DIR/kb" "$SCRIPT_DIR/kb-cli"; do
  if [[ -f "$candidate" ]]; then
    SOURCE_BIN="$candidate"
    break
  fi
done

if [[ -z "$SOURCE_BIN" ]]; then
  printf '[kb-cli] No installable binary found (kb or kb-cli)\n' >&2
  exit 1
fi

mkdir -p "$INSTALL_DIR"
install -m 0755 "$SOURCE_BIN" "$INSTALL_DIR/$BIN_NAME"
log "Installed binary: $INSTALL_DIR/$BIN_NAME"

PATH_LINE=''
if [[ "$INSTALL_DIR" == "$HOME/.local/bin" ]]; then
  PATH_LINE='export PATH="$HOME/.local/bin:$PATH"'
else
  ESCAPED_INSTALL_DIR="${INSTALL_DIR//\"/\\\"}"
  PATH_LINE="export PATH=\"$ESCAPED_INSTALL_DIR:\$PATH\""
fi

append_path_line() {
  local profile_file="$1"
  if [[ -f "$profile_file" ]]; then
    if grep -Fqx "$PATH_LINE" "$profile_file"; then
      return
    fi
  fi
  printf '\n%s\n' "$PATH_LINE" >> "$profile_file"
  log "Updated PATH in: $profile_file"
}

case ":$PATH:" in
  *":$INSTALL_DIR:"*)
    log "Current PATH already contains install directory"
    ;;
  *)
    profile_updated=0
    for profile in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile"; do
      if [[ -f "$profile" ]]; then
        append_path_line "$profile"
        profile_updated=1
      fi
    done
    if [[ "$profile_updated" -eq 0 ]]; then
      append_path_line "$HOME/.profile"
    fi
    ;;
esac

printf '\nInstall complete.\n'
printf 'Try: kb --help\n'
printf 'If kb is not recognized, run "source ~/.profile" or reopen terminal.\n'
