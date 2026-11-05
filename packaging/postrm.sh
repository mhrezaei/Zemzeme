#!/bin/sh
set -e
if command -v udevadm >/dev/null; then
  udevadm control --reload-rules || true
fi
exit 0
