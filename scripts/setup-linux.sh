#!/usr/bin/env bash
# Lets my-whisper press the paste shortcut through a virtual keyboard.
#
# Grants the logged-in desktop user access to /dev/uinput (via systemd's
# "uaccess" tag, the same mechanism used for game controllers). No group
# change or re-login needed.
#
# Run once:  sudo ./scripts/setup-linux.sh
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
  echo "Run with sudo: sudo $0" >&2
  exit 1
fi

RULE=/etc/udev/rules.d/70-my-whisper-uinput.rules

cat > "$RULE" <<'EOF'
# my-whisper: allow the active desktop user to create a virtual keyboard
KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput"
EOF

modprobe uinput
udevadm control --reload-rules
udevadm trigger --name-match=uinput --action=change

echo "Wrote $RULE"
ls -l /dev/uinput
getfacl -p /dev/uinput 2>/dev/null | grep '^user:' || true
echo "Done. Restart my-whisper."
