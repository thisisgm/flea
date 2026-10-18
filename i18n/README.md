# Flea catalogs

`en/flea.json` defines flat keys, `%1` through `%N` arguments, and cardinal plural objects with an `other` form. Other catalogs use the same keys and arguments; their plural category names can differ. Empty or missing translations fall back to English per string. The English plural rule applies when the English string supplies that fallback.

Run `python3 -B tools/flea-i18n-manifest` after reviewing catalog changes. The manifest maps each language to `flea:key` entries containing the first 12 hexadecimal characters of the English SHA-256 hash, matching Omarchy's hash convention. The `en` section is the source index. Hashing a plural object uses sorted JSON keys with Python's default separators and unescaped Unicode, as Omarchy does.

`python3 -B tools/flea-i18n-manifest --check` checks hashes, key parity, value shapes, and argument parity. Shared desktop words can be looked up directly as `omarchy:shell.common.search`, `omarchy:menu.apps.label`, or `omarchy:keybindings.Close window`. A Flea string can also contain an entire reference. Reference values in catalogs are checked against Omarchy's English source; use `--omarchy-path DIR` for an uninstalled checkout and `--reference REFERENCE` for references used directly by callers.

`python3 -B tools/flea-i18n-roster [locales.json]` fails if any Omarchy language has no Flea catalog. During extraction, `--warn-only` reports those gaps without failing. This does not turn unreadable or malformed rosters into successes.

The core reads `$OMARCHY_PATH/default/i18n/locales.json` and `$OMARCHY_LANGUAGE_FILE` or `${XDG_CONFIG_HOME:-$HOME/.config}/omarchy/language`. Omitting or invalidating the language selects English. `Paths::catalogs` lists catalog roots in priority order; defaults search `$FLEA_I18N`, executable-relative and system data directories, then source-tree locations. Call `I18n::poll_changed` from the caller's event loop to follow changes without a core thread.

`tr` reads a string, `tr_args` substitutes caller arguments, and `trn` inserts the formatted count as `%1`. In `trn_args`, additional arguments start at `%2`. `Locale` supplies number, grouped count, decimal size and local date formatting. Dates keep `YYYY-MM-DD HH:MM` and use the roster's digits. glibc locale handles do not change the process locale; absent generated locale data falls back to conventional number grouping and the C locale for fixed-form dates.

The plural equality test uses installed Omarchy data when available and otherwise the pinned Omarchy `bf9ee5c` fixtures in `src/i18n/fixtures`. It compares all roster codes for counts 0 through 10000 against `plural.jq`. A missing `jq` prints an explicit skip when test output is visible.

Runtime catalog and roster documents are limited to 1 MiB, 16384 total object members and array elements, and 32 levels of nesting. Duplicate members count toward the limit and the last value wins. Parsing uses a forward cursor and hashed object insertion; it never scans prior members for each key. Files exceeding a limit are refused with a diagnostic and ordinary English fallback. The release regression loads a representative complete-language budget of 4096 labels, errors and plural messages through `I18n::load` three times, with a 250 ms limit per run; it also checks prompt refusal of a 3920001-byte catalog.
