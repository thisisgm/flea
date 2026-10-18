#!/usr/bin/env bash
# Owner deadlines, held-row recovery and release controls through the real TUI.
set -euo pipefail
/bin/bash "$(dirname "$0")/lib/tui-pty.sh" tui-owner-stall tui_owner_stall.py control deadline recovery
printf 'PASS tui-owner-stall: held-stat deadline, healthy navigation, release control, clean TTY quit\n'
