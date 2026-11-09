# CLI compatibility with 0.3.12

The reference is tag `v0.3.12` (`f21789e56`), built from source. Each row below is exercised by `tests/cli-contract.sh`. Exact exit status, stdout and stderr live in `tests/compat/cli-v0312.json`, captured from that binary. Only sandbox, executable and repository paths use substitution tokens; version output must equal the current package version plus one newline. Every other byte is compared directly, including whitespace and stderr usage text.

GUI rows use recording `qs` and `flea-gui` children. They assert the launch environment carrying path, selection, picker request, reply file and unlock device. The candidate suite repeats the inventory with `FLEA_FRONTEND=gpui`; `flea-gui` takes path and selection only as `--select <target> -- <path>` arguments and the chooser reply file as `--pick <reply>`, so its recorder maps them onto the same fields and fails if either arrives through the environment. Child implementation argv are private; public `flea` argv remain unchanged.

| Case | Invocation after `flea` | Exit | Stdout |
| --- | --- | --- | --- |
| `version` | `--version` | 0 | @VERSION@ |
| `gui-desktop` | `--gui <sandbox>/files` | 0 | recorded child environment |
| `bare-directory` | `<sandbox>/files` | 0 | recorded child environment |
| `bare-file` | `"<sandbox>/files/a b.txt"` | 0 | recorded child environment |
| `bare-relative` | `files` | 0 | recorded child environment |
| `select-path` | `--select "<sandbox>/files/a b.txt"` | 0 | recorded child environment |
| `select-uri` | `--select file://<sandbox>/files/a%20b.txt` | 0 | recorded child environment |
| `select-print` | `--print-target --select "<sandbox>/files/a b.txt"` | 0 | @ROOT@/files @ROOT@/files/a b.txt |
| `gui-no-display` | `--gui` | 2 | empty |
| `gui-conflict` | `--gui --tui` | 2 | empty |
| `tui-pipe` | `--tui <sandbox>/files` | 2 | empty |
| `cwd-file` | `--tui --cwd-file <sandbox>/cwd` | 2 | empty |
| `backend` | `--backend` | 0 | empty |
| `open-folder` | `--open <sandbox>/files` | 3 | empty |
| `open-file` | `--open "<sandbox>/files/a b.txt"` | 0 | empty |
| `terminal` | `--terminal <sandbox>/files` | 0 | empty |
| `pick` | `--pick <sandbox>/reply.json` | 0 | recorded child environment |
| `pick-no-request` | `--pick <sandbox>/reply.json` | 2 | empty |
| `pick-no-display` | `--pick <sandbox>/reply.json` | 2 | empty |
| `unlock` | `--unlock /dev/contract` | 0 | recorded child environment |
| `ui-state` | `--ui-state` | 0 | full JSON document |
| `ui-state-write` | `--ui-state {"view":"grid"}` | 0 | full JSON document |
| `favourites` | `--favourites {"op":"inspect","indices":[]}` | 0 | { \n   "statuses": [] \n } |
| `default` | `--default` | 3 | empty |
| `default-off` | `--default off` | 1 | exact bytes in golden record |
| `default-alias` | `--youleftmeforstrata` | 1 | exact bytes in golden record |
| `picker-claim` | `--picker` | 1 | empty |
| `picker-off` | `--picker off` | 1 | exact bytes in golden record |
| `prewarm` | `--prewarm <sandbox>/files 2 <sandbox>/prewarm` | 0 | empty |
| `launch-warm` | `--launch-warm - - -` | 0 | empty |
| `clip-empty` | `--clip set copy` | 2 | empty |
| `clip-clear` | `--clip clear ` | 2 | empty |
| `clip-own` | `--clip-own` | 2 | empty |
| `run` | `--run "<sandbox>/files/a b.txt" 0 0 0 0 0` | 2 | empty |
| `run-terminal` | `--run-terminal "<sandbox>/files/a b.txt" 0 0 0 0 0` | 2 | empty |
| `open-handler` | `--open-handler "<sandbox>/files/a b.txt" bad 0 0 0 0 0` | 6 | empty |
| `open-text` | `--open-text "<sandbox>/files/a b.txt"` | 2 | empty |
| `flash` | `flash choose <sandbox>/files` | 2 | empty |
| `update-handoff` | `--update` | 0 | empty |
| `update-check` | `--update check` | 3 | unchecked unowned - - |
| `clip-get` | `--clip get` | 2 | {"t":"clip","op":"get","ok":false,"error":"WAYLAND_DISPLAY is not set, so there is no clipboard to use"} |
| `clip-clear-token` | `--clip clear contract-token` | 2 | {"t":"clip","op":"clear","ok":false,"error":"WAYLAND_DISPLAY is not set, so there is no clipboard to use"} |
| `figure-store` | `--figure-store` | 0 | empty |
| `unknown` | `--not-a-mode` | 2 | empty |
| `usage-version` | `--version extra extra` | 2 | empty |
| `usage-prewarm` | `--prewarm` | 2 | empty |
| `usage-launch-warm` | `--launch-warm` | 2 | empty |
| `usage-open` | `--open` | 2 | empty |
| `usage-terminal` | `--terminal` | 2 | empty |
| `usage-unlock` | `--unlock` | 2 | empty |
| `usage-update` | `--update extra extra` | 2 | empty |
| `usage-default` | `--default extra extra` | 2 | empty |
| `usage-youleftmeforstrata` | `--youleftmeforstrata extra extra` | 2 | empty |
| `usage-picker` | `--picker extra extra` | 2 | empty |
| `usage-pick` | `--pick` | 2 | empty |
| `usage-clip` | `--clip` | 2 | empty |
| `usage-clip-own` | `--clip-own extra extra` | 2 | empty |
| `usage-ui-state` | `--ui-state extra extra` | 2 | empty |
| `usage-favourites` | `--favourites` | 2 | empty |
| `usage-run` | `--run` | 2 | empty |
| `usage-run-terminal` | `--run-terminal` | 2 | empty |
| `usage-open-text` | `--open-text` | 2 | empty |
| `usage-open-handler` | `--open-handler` | 2 | empty |
| `usage-select` | `--select` | 2 | empty |
| `usage-print-target` | `--print-target` | 2 | empty |
| `usage-thumb-worker` | `--thumb-worker extra extra` | 2 | empty |
| `usage-figure-helper` | `--figure-helper extra extra` | 2 | empty |
| `usage-figure-compile` | `--figure-compile extra extra` | 2 | empty |
| `usage-figure-store` | `--figure-store extra extra` | 2 | empty |
| `shelf-drag-begin` | `shelf drag-begin` | 2 | empty |
| `shelf-size` | `shelf size` | 2 | empty |
| `shelf-forget` | `shelf forget` | 2 | empty |
| `shelf-add` | `shelf add` | 2 | empty |
| `shelf-pin` | `shelf pin` | 2 | empty |
| `shelf-unpin` | `shelf unpin` | 2 | empty |
| `shelf-order` | `shelf order` | 2 | empty |
| `shelf-thumb` | `shelf thumb` | 2 | empty |
| `shelf-open` | `shelf open` | 2 | empty |
| `shelf-move` | `shelf move` | 2 | empty |
| `shelf-copy` | `shelf copy` | 2 | empty |
| `shelf-zip` | `shelf zip` | 2 | empty |
| `shelf-paths` | `shelf paths` | 0 | empty |
| `shelf-clear` | `shelf clear` | 0 | 0 |
| `shelf-restore` | `shelf restore` | 2 | empty |
| `shelf-piles` | `shelf piles` | 0 | empty |
| `shelf-toggle` | `shelf toggle` | 0 | empty |
| `shelf-bad` | `shelf bad` | 2 | empty |

The desktop entry is `flea --gui %f`. FileManager1 `ShowItems` deduplicates by parent and invokes `flea --select <path>`; `ShowFolders` invokes `flea <dir>`. Its URI validation and NotSupported property request remain covered by `tests/filemanager1.sh`. The helper source and registration are unchanged.

`tools/flea-portal` invokes `flea --pick <reply.json>` with the request JSON in `FLEA_PICKER`. The chooser writes its answer to the reply file before exiting. Stdout is not its answer channel. The helper returns D-Bus response 0 with URIs, 1 for user cancellation, or 2 for failure. `packaging/flea.portal` and both service helpers remain unchanged. `tests/portal.sh` owns the real D-Bus request/reply checks.

`--cwd-file` is not parsed by 0.3.12. Its existing status 2 and usage output are deliberately preserved. There is no new flag of that name in this unit. Bare file arguments retain the old launcher behavior; unlike `--select`, they do not implicitly become a parent/selection pair.

`--help` and `-h` are launch flags: they print the usage text on stdout and exit 0. An argv that `ipc`, `--action`, `--backend` or a private worker claims first never reads them, so `flea --backend --help` still runs the backend. `-V` is only the whole command `flea -V`, which prints the version as `flea --version` does; anywhere else before `--` it is an unknown flag. In the launch arguments, a word that starts with `-` and is not a launch flag this file names is an unknown flag with status 2, never a start folder, so `flea --tui -V` and `flea -x` are refused. After `--`, every launch argument is a path, so `flea --tui -- -h` opens a folder named `-h`. `--version` takes nothing: anywhere but alone, even after `--`, it is refused with status 2. A plain word stays a path, so a folder named `help` still opens. `flea --tui` refuses `TERM=dumb` with status 2, and `NO_COLOR` set to any non-empty value draws the terminal interface without colour.

`--backend` wins over ordinary flags; `--version` is checked first. Private thumbnail/figure workers retain their argv. Malformed private-worker shapes are included; valid figure SVG persistence is covered by the state fixture. Valid multimedia workers, clipboard ownership on a live compositor, update networking, interactive TUI input, and external updater handoff require their existing subsystem/native suites. This inventory does not claim to simulate those services.

`tests/cli-contract-mutations.sh` builds eight temporary source mutants: usage exit, backend dispatch, GUI mode, TUI pipe refusal, selection, picker reply path, open-directory exit and state stdout. Each must fail its corresponding byte comparison. No production source is edited by the mutation suite.
