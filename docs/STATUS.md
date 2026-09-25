# Status

The refresh review fix for [PR #197](https://github.com/thisisgm/flea/pull/197) preserves an open rename editor through context-menu focus changes. Refresh refuses while the editor is open, a collision decision is pending or a committed rename is still finishing, including in search results.

Run `tests/refresh-menu.sh` for the real QML menu/editor focus regression, `tests/js.sh refresh` for the refresh and collision guards, and `tests/run-all.sh` for the headless battery. Set `FLEA_FIXTURE_ROOT` to a writable fixture directory outside the home directory on machines without `/home/flea-sandbox`.

The native `tests/ui.sh refresh` check requires `omarchy-drive`. Open review work is listed by `gh pr view 197 --repo thisisgm/flea --comments`.

Validation on 2026-09-25: the menu-focus, search-collision and search-rename regressions fail before their fixes and pass after them, and `tests/refresh-menu.sh` fails against a `manual()` that refuses silently over search results. The full JavaScript battery, the debug Rust unit suite with warnings denied, QML lint and file-budget checks pass. With `QT_QPA_PLATFORMTHEME=basic`, shell loading also passes. The headless runner remains red in ops, protocol, thumbs, thumbs-exec, uistate and media, matching the existing baseline failures, and in filemanager1, whose private bus did not start on this machine and which fails the same way on the PR head before this change. The thumbnail suites lack their media fixture. The menu test exercises real QML focus offscreen; no native pointer-driven verification is claimed.
