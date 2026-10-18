#!/usr/bin/env bash
# Real TUI detail, dates and sort replies in a detached private PTY.
set -euo pipefail
exec /bin/bash "$(dirname "$0")/lib/tui-pty.sh" tui-detail tui_detail.py
