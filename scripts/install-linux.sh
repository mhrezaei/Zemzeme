#!/usr/bin/env bash
# Install my-whisper for the current user (no sudo):
#   ~/.local/bin/my-whisper, app-menu entry, icon, start at login.
# The app registers its GNOME shortcut (Super+Alt+Space) itself on start.
#
#   ./scripts/install-linux.sh            build (release) and install
#   ./scripts/install-linux.sh --no-build install the existing release build
#   ./scripts/install-linux.sh --uninstall
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$HOME/.local/bin/my-whisper"
DESKTOP="$HOME/.local/share/applications/my-whisper.desktop"
ICON="$HOME/.local/share/icons/hicolor/128x128/apps/my-whisper.png"
AUTOSTART="${XDG_CONFIG_HOME:-$HOME/.config}/autostart/my-whisper.desktop"
KEYPATH=/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/my-whisper/

stop_running() {
  if pgrep -x my-whisper >/dev/null; then
    echo "Stopping running my-whisper…"
    pkill -x my-whisper || true
    sleep 1
  fi
}

if [[ "${1:-}" == "--uninstall" ]]; then
  stop_running
  rm -f "$BIN" "$DESKTOP" "$ICON" "$AUTOSTART"
  # Remove our entry from GNOME's custom shortcuts list.
  list=$(gsettings get org.gnome.settings-daemon.plugins.media-keys custom-keybindings)
  new=$(python3 -c "import ast,sys; l=ast.literal_eval(sys.argv[1].replace('@as ','')); print([p for p in l if p!=sys.argv[2]])" "$list" "$KEYPATH")
  gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "$new"
  echo "Uninstalled. Settings (~/.config/dev.hadi.mywhisper) and the keyring entry were kept."
  exit 0
fi

if [[ "${1:-}" != "--no-build" ]]; then
  echo "Building release…"
  NODE_BIN="$(ls -d "$HOME"/.nvm/versions/node/v2[4-9]*/bin 2>/dev/null | sort -V | tail -1 || true)"
  export PATH="${NODE_BIN:+$NODE_BIN:}$HOME/.cargo/bin:$PATH"
  (cd "$ROOT" && npm run tauri build -- --no-bundle)
fi

SRC="$ROOT/src-tauri/target/release/my-whisper"
[[ -x "$SRC" ]] || { echo "No release build at $SRC" >&2; exit 1; }

stop_running
install -Dm755 "$SRC" "$BIN"
install -Dm644 "$ROOT/src-tauri/icons/128x128.png" "$ICON"

ENTRY="[Desktop Entry]
Type=Application
Name=my-whisper
Comment=Speech to text at your cursor
Exec=$BIN
Icon=my-whisper
Terminal=false
Categories=Utility;Accessibility;
StartupNotify=false"

mkdir -p "$(dirname "$DESKTOP")" "$(dirname "$AUTOSTART")"
printf '%s\n' "$ENTRY" > "$DESKTOP"
printf '%s\nX-GNOME-Autostart-enabled=true\n' "$ENTRY" > "$AUTOSTART"
update-desktop-database -q "$(dirname "$DESKTOP")" 2>/dev/null || true

# Start detached from this terminal.
setsid "$BIN" >/dev/null 2>&1 < /dev/null &
sleep 2

echo "Installed $BIN"
echo "Shortcut: $(gsettings get org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:$KEYPATH binding)"
echo "        → $(gsettings get org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:$KEYPATH command)"
echo "Starts at login: $AUTOSTART"
