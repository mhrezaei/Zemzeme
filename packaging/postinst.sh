#!/bin/sh
# Apply the uinput rule now so pasting works without a reboot.
set -e
modprobe uinput 2>/dev/null || true
if command -v udevadm >/dev/null; then
  udevadm control --reload-rules || true
  udevadm trigger --name-match=uinput --action=change || true
fi
exit 0
