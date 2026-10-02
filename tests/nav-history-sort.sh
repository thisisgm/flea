#!/usr/bin/env bash
# An owed Back cursor waiting for a tab's re-sorted rows against a filter opened meanwhile, through the
# real window and backend: tests/nav-history.sh runs tests/nav-history-sort.qml with the same fixture,
# guard, strict receipt and cleanup. NAV_HISTORY_KEEP=1 keeps the fixture as it does there.
exec "$(dirname "$0")/nav-history.sh" sort
