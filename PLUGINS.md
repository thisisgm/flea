# Flea plugins

## 1. What a plugin is

A third-party plugin is a folder containing `manifest.json`, an executable and optional assets.
Flea installs it at `$XDG_CONFIG_HOME/flea/plugins/<id>/`.
If `XDG_CONFIG_HOME` is unset or empty, the base is `$HOME/.config`.
Use an absolute path when setting `XDG_CONFIG_HOME`.

In Flea 0.4.0, GPUI shows third-party plugins in Settings > Plugins.
Each plugin has a switch and a page containing its description, generated settings, declared access and source.
A supervised executable handles its declared contribution requests over the versioned JSON-lines protocol.
Flea owns the surfaces and draws every label as plain text.
The protocol contract is [docs/plugin-protocol.json](docs/plugin-protocol.json).

Plugins run as separate processes. They do not link to GPUI or load Rust or QML UI code into Flea.
Flea draws their settings using its own controls.

## 2. Quick start

This example installs a plugin, checks its startup exchange and shows its GPUI settings page.
It requires Python 3. Use a folder outside the installed plugin store as the source:

```sh
mkdir -p "$HOME/flea-plugin-hello/bin"
cd "$HOME/flea-plugin-hello"
```

Create `manifest.json` with these exact contents:

```json manifest=hello/manifest.json
{
  "schemaVersion": 1,
  "id": "example.hello",
  "name": "Hello",
  "version": "0.1.0",
  "author": "Example author",
  "description": "A startup and settings example.",
  "kinds": ["menu-action"],
  "entryPoints": {"run": "bin/run"},
  "menuActions": []
}
```

The kind declaration satisfies the manifest contract. This example has no menu contribution.
Create the executable `bin/run`:

```python file=hello/bin/run
#!/usr/bin/env python3
import json
import sys

for line in sys.stdin:
    message = json.loads(line)
    op = message.get("op")
    if op == "cancel":
        break
    if op == "hello":
        answer = {"op": "ready", "protocolVersion": 1}
    elif op == "settings":
        answer = {"op": "done"}
    else:
        answer = {"op": "error", "message": "This example has no contribution actions."}
    print(json.dumps(answer), flush=True)
```

Make it executable, validate it, add it and enable the installed copy:

```sh
chmod 755 "$HOME/flea-plugin-hello/bin/run"
flea plugin validate "$HOME/flea-plugin-hello"
flea plugin add "$HOME/flea-plugin-hello"
flea plugin enable example.hello
flea plugin list
flea plugin list --json
```

Successful validate, add and enable commands are silent and exit zero.
Each list line holds the id, the state, the version and the comma-separated kinds, separated by tabs.
Hello's line reads `example.hello`, `enabled`, `0.1.0` and `menu-action`.
Enabling can succeed while the startup check fails. Only the process that ran the check remembers that failure,
so a later `flea plugin list` reads `failed` only for a problem it finds on disk, such as a missing executable.
In a running Flea, a failed start shows as `failed to start` on the plugin's row in Settings > Plugins.
Open Flea's GPUI frontend:

```sh
flea --gui
```

Open Settings > Plugins. If Hello is absent in an already running window, restart Flea after the CLI commands.
The Hello row has an enabled checkbox. Enter opens its page; Space toggles its switch.
The page shows the description and source folder, plus Remove and Back to Plugins.

To finish the example:

```sh
flea plugin disable example.hello
flea plugin remove example.hello
```

Remove deletes the installed copy and its saved settings. The development source folder remains.

## 3. Manifest reference

Use UTF-8 JSON with an object at the root. Keep `manifest.json` a regular file of at most 65,536 bytes.
Parse failures select `plugins.invalid.json`.
The disk reader rejects unreadable, non-regular, oversized or non-UTF-8 files before parsing.
Validate and add report `plugins.error.no_manifest` for those files.
An installed folder instead appears with `plugins.invalid.manifest`.

Unknown fields are ignored. A valid manifest can therefore contain fields that have no effect.
Every declared kind requires its validated block. A block for an undeclared kind is refused.
An invalid kind block selects `plugins.invalid.block`.

| Field | Type and requirement | Limits and refusal |
| --- | --- | --- |
| `schemaVersion` | Required number, exactly `1`. | `plugins.invalid.schema_version`. |
| `id` | Required string. At least two nonempty dot-separated segments. | At most 64 ASCII bytes. Segment characters: lowercase `a-z`, digits, `_`, `-`. Invalid syntax: `plugins.invalid.id`. The `flea.` prefix is reserved: `plugins.invalid.reserved`. |
| `name` | Required string with at least one non-whitespace character. | At most 512 Unicode characters. Empty or wrong type: `plugins.invalid.missing`; too long: `plugins.invalid.long`. |
| `version` | Required string. No semantic-version parsing. | At most 64 Unicode characters. Wrong type: `plugins.invalid.missing`; too long: `plugins.invalid.long`. |
| `author` | Required string. | At most 512 Unicode characters; empty is accepted. `plugins.invalid.missing` or `plugins.invalid.long`. |
| `description` | Required string. | At most 512 Unicode characters; empty is accepted. `plugins.invalid.missing` or `plugins.invalid.long`. |
| `license` | Optional string, default empty. | At most 512 Unicode characters. `plugins.invalid.missing` for a supplied wrong type, or `plugins.invalid.long`. |
| `kinds` | Required nonempty array of recognized strings. The startup example uses `menu-action`. | Missing, empty or non-string entries: `plugins.invalid.missing`; unknown token: `plugins.invalid.kind`. |
| Kind block | Required for each declared kind. The example's block is `menuActions`. | Absent block: `plugins.invalid.block`. Its shape and fields must satisfy the kind contract below. |
| `entryPoints.run` | Required string naming one executable relative to the plugin folder. It is a path, not a shell command or argument list. | Nonempty; no leading `/`, NUL or `..` path component. Missing or wrong type: `plugins.invalid.missing`; unsafe path: `plugins.invalid.run`. |
| `mark` | Optional relative path string, default empty. | At most 512 Unicode characters. Nonempty paths follow the same lexical path rules as `run`: `plugins.invalid.run`. Wrong type: `plugins.invalid.missing`; too long: `plugins.invalid.long`. |
| `access` | Optional array of strings, default empty. | Wrong array or element type: `plugins.invalid.missing`. Scope names and their lengths are not validated individually. |
| `schema` | Optional array of settings field objects, default empty. | Wrong container type: `plugins.invalid.missing`; invalid or duplicate fields: `plugins.invalid.field`. |

The common text reader removes trailing newline characters from `id`, `name`, `version`, `author`,
`description`, `license` and `mark`, after checking the 512-character text limit.
The id and version limits then apply to the resulting value.
There is no separate length check on `entryPoints.run` beyond the manifest's total byte limit.
`run` must resolve to a regular executable file inside the plugin's canonical folder.
Otherwise validate refuses with `plugins.invalid.exec`, and an enabled plugin lists that code as its failure.
Add does not check the executable, and enable reports the failure only through the listing.
Keep the folder basename equal to the id. Add does this automatically.

### Glyphs and SVG marks

`glyph` belongs inside a kind block or one of its array entries. There is no top-level `glyph` field.
For the Settings row, Flea uses the first string-valued block glyph it finds, in the validator's kind order.
GPUI's Settings glyph names are:

```text
shelf localsend tailscale dropbox maximize image film list download plus
plug chevron-right folder file
```

A menu row's mark name, listed under menu-action below, also draws on the Settings row. For example, Checksums'
`hash` gives its row the same mark as its Verify Checksum… menu row.
An unknown glyph draws no mark. Glyph names are not manifest validation errors.
A plugin can instead set the top-level `mark` path to its own SVG.
The Settings view loads the file once, in the background, with a 65,536-byte limit.
It paints the SVG as an alpha mask in the row's ink, at 16 pixels inside a 19-pixel slot.
Use a self-contained SVG with a `viewBox`, transparent background and simple opaque shapes.
Colors in the SVG do not provide a multicolor row icon.
The manifest validator does not parse SVG, check its extension or require the mark file to exist.
An absent or unreadable mark falls back to the named glyph; malformed loaded SVG can draw nothing.
Restart Flea to inspect a changed asset. Reopening Settings can retain the view's cached mark.

### Declared access

Access entries supply the facts shown under ACCESS on the plugin's page.
They are declarations, not an OS permission grant or sandbox policy.
The process already has the user's normal permissions, including when `access` is empty.
File actions receive only the selection captured when their menu opens. Discovery requests receive no file paths.

| Entry | What the Settings page describes |
| --- | --- |
| `selection` | Reading the selected files. |
| `write:selection` | Changing the selected files. |
| `folder` | Reading the open folder. |
| `network` | Using the network. |
| `notify` | Showing notifications. |
| `exec:<program>` | Running the named program. Multiple entries become one Runs line. |
| `shelf:pile` | Reading the files in the Shelf pile. |
| `shelf:bar` | Showing the Shelf on the bar. |
| `shelf:captures` | Collecting screenshots and recordings. |

An unknown entry is shown in the sentence "Declares access to <entry>". It does not add capabilities.
An `exec:` entry with no program name is not shown.
Declare actual behavior accurately, and do not store credentials in these strings or in settings.

### Settings fields and clamping

Every field needs a unique `key`, a nonblank string `label` and a supported string `type`.
Keys contain only ASCII letters, digits, `_` and `-`, with length 1 to 64 bytes.
An optional string `hint` appears below the field. Invalid fields select `plugins.invalid.field`.
Labels, hints, units, option labels and placeholders have no separate length cap beyond the manifest limit.

| Type | Definition | Stored-value behavior |
| --- | --- | --- |
| `boolean` | Required Boolean `defaultValue`. | A Boolean is kept; another type or a missing value uses the default. |
| `integer` | Required whole-number `min`, `max` and `defaultValue`. `min <= defaultValue <= max`. Optional `step` defaults to 1 when absent or not a supported whole number; a whole step must be at least 1. Optional string `unit`. | Finite numbers clamp to the endpoints or round to the nearest step measured from `min`. Half steps round upward inside the range. Both endpoints remain reachable even off the step grid. Wrong types use the default. |
| `string` | Optional string `defaultValue` and `placeholder`, each defaulting to empty when absent or wrong type. Defaults truncate to 4,096 Unicode characters. | A string of at most 4,096 Unicode characters without NUL is kept. Otherwise the default is used. |
| `choice` | Required `options` array with 1 to 16 objects. Each needs nonempty string `value` and string `label`. Required string `defaultValue` must match one option value. | An exact option value is kept. Any other value or type uses the default. |

Whole numbers in field definitions must be finite, integral and have absolute value below `9e15`.
Integer defaults need not lie on the step grid. Wrong stored types return the declared default directly.
Choice values should be unique for predictable controls, although the validator does not enforce uniqueness.

Replace the quick-start manifest with this complete settings example to exercise all four controls:

```json manifest=settings/manifest.json
{
  "schemaVersion": 1,
  "id": "example.settings",
  "name": "Settings example",
  "version": "0.1.0",
  "author": "Example author",
  "license": "MIT",
  "description": "Demonstrates generated GPUI settings.",
  "kinds": ["menu-action"],
  "entryPoints": {"run": "bin/run"},
  "menuActions": [],
  "access": [],
  "schema": [
    {"key": "verbose", "label": "Verbose", "type": "boolean", "defaultValue": false},
    {"key": "timeout", "label": "Timeout", "type": "integer", "min": 2, "max": 60,
     "step": 5, "defaultValue": 12, "unit": "s", "hint": "Rounded to the nearest step."},
    {"key": "prefix", "label": "Prefix", "type": "string", "defaultValue": "Hello",
     "placeholder": "Greeting"},
    {"key": "format", "label": "Format", "type": "choice",
     "options": [{"value": "short", "label": "Short"}, {"value": "long", "label": "Long"}],
     "defaultValue": "short"}
  ]
}
```

Reuse `bin/run` from the quick start, validate, add and enable with the new id.
Flea disables the schema controls when the plugin is off.
Saved values live in `$XDG_STATE_HOME/flea/ui.json` (`~/.local/state/flea/ui.json` when the variable is unset),
under `plugins.<id>.settings`.
Flea clamps values both when writing settings through its plugin API and when listing them.
A value of the wrong type is not refused: it is stored as the field's default.
After a change, an active session receives one `settings` message with the clamped values, then Flea stops it.
The next request starts a new process, and its hello carries the new settings.
A plugin that fails to answer the `settings` message is marked failed.

## 4. Contribution kinds

Each declared kind adds data to an existing Flea surface. Flea draws its controls; plugins do not supply UI code.
A disabled or failed plugin contributes no rows. Kind blocks are retained through the backend wire.
Each kind has a working example under [examples/plugins](examples/plugins), described in the kind reference below.

| Kind | Required block | Surface |
| --- | --- | --- |
| `send-target` | `sendTargets` object with `label` | A separate file-menu row for the provider, with its targets. |
| `menu-action` | `menuActions` array | File or background menu rows whose selection matches `when`. |
| `rail-section` | `railSection` object with `title` | A section after Flea's built-in rail sections. |
| `place` | `place` object with `label` | A rail row for a local folder or plugin-owned listing. |
| `previewer` | `previewer` object with `mime` and `output` | The preview column and Quick Look, through the thumbnail sandbox. |
| `shelf` | `shelf` object with `omarchyPlugin` | The named Omarchy bar plugin's switch and generated Settings page. |

Menu actions use an `id` and `label`. Optional `menus` contains `file`, `background`, or both; the default is `file`.
`when.minItems` defaults to 1, `maxItems` defaults to 512, and both must be whole numbers from 0 through 512.
`maxItems` must be at least `minItems`. `folders` defaults to false, which hides the row when any selected item is a
folder. If supplied, `mime` is an array of at most 64 nonempty patterns of at most 256 bytes; every selected item must
match. Each menu block contains at most 64 actions with distinct ids. An action id cannot contain `:`.
An empty array declares no rows.

### Menu actions that ask for text

An action can declare one text input. Flea draws the title, field, Cancel and submit button with its ordinary card
controls. The plugin supplies data, never UI code:

```json
{
  "id": "verify",
  "label": "Verify Checksum…",
  "input": {
    "key": "checksum",
    "title": "Verify Checksum",
    "label": "Checksum",
    "submit": "Verify",
    "placeholder": "",
    "maxChars": 128
  }
}
```

`key`, `title`, `label` and `submit` are required. The key uses 1 to 64 ASCII letters, digits, underscores or hyphens.
Title and label are limited to 80 characters, submit to 24, and placeholder to 256.
`maxChars` is a whole number from 1 through 4,096, defaulting to 4,096.
The submitted value must be nonblank, contain no control characters and fit the declared limit.

No run request is sent while the card is open. Cancel, Escape and window close discard the draft.
After submission Flea validates the original captured selection again and checks that the action is still available.
It does not substitute a newer selection. The run request adds exactly one `input` key matching the declaration:

```json
{"op":"run","action":"verify","paths":["/selected/notes.txt"],"settings":{},"input":{"checksum":"expected SHA-256 sum"}}
```

The executable compares that value with its computed sum and answers `done` or an ordinary operation `error`.
An operation error does not mark a healthy executable as failed.

### Keys for menu actions

A user can bind a key to any enabled plugin's menu action, or to a built-in action, in
`$XDG_CONFIG_HOME/flea/keys.toml` (`~/.config/flea/keys.toml` when the variable is unset). The file uses the
table shape of Flea's own `keys.toml`. A `[[ctrl]]`, `[[ctrlshift]]`, `[[shift]]`, `[[code]]` or `[[text]]` block
holds `key` and `action` and applies to every preset. A `[[preset]]` block adds `name` (`default`, `vim`, `mac`,
`windows` or `all`) and `mods` (`none`, `text`, `ctrl`, `shift`, `ctrlshift`, `alt`, `super`, `supershift` or
`superalt`). A plugin action is named `plugin-id:action-id`. This binds Ctrl+Shift+K to the checksums example's
Verify Checksum action:

```toml keys=keys.toml
[[ctrlshift]]
key = "K"
action = "example.checksums:sha256"
```

Flea reads the file at start and again within two seconds of a change. A bound key runs the action exactly as
choosing its menu row does, on the focused pane's selection or cursor row, with the same `when` check, input card
and refusals. An action offered only in the background menu runs on the open folder.

Settings > Keys lists each live binding under Your Keys, with the plugin's own label. A binding whose chord the
active preset already uses, that repeats an earlier chord in the file, or that names no built-in or installed plugin action stays
unbound. Settings > Keys and stderr each state why in one line. Ctrl+Q and Ctrl+P are reserved. A binding to a
disabled or removed plugin is dropped from the list and from the keyboard until the plugin is enabled again.

### Kind reference

Each kind below except shelf has a complete working plugin under [examples/plugins](examples/plugins).
Its manifest is shown in full. The exchange after it shows, in order, each request Flea sends after `hello` and every line the example answers,
including progress. The repository documentation test runs every exchange against the example's own executable and
compares each answer, and it loads each example through Flea's real plugin runtime. `/home/me/notes.txt` holds the
four bytes `test`. Flea sends each request only to a plugin that declares the kind, only while the plugin is enabled
and has not failed.

To try an example, add its folder, then enable it:

```sh
flea plugin validate "$PWD/examples/plugins/checksums"
flea plugin add "$PWD/examples/plugins/checksums"
flea plugin enable example.checksums
```

#### send-target

Flea asks for targets when a file menu opens, at most once per `refreshSec` seconds, and draws one row per target
under the provider's own row. A `refreshSec` outside 1 to 3,600 is clamped; the default is 10. The provider row appears
in the file menu only, never in the background menu. Its `glyph` takes a menu mark name (see menu-action) and defaults
to `network`. Each target needs `id`, `label` and `state` strings. A missing one, one longer than 4,096 bytes or
holding a NUL, or an empty or repeated `id` fails the whole answer as described under Runtime protocol. Flea drops a
target whose `label` or `state` is empty, or whose `id`, `label` or `state` is longer than 512 characters or holds
another control character, and keeps the others. `state` is `ready` or any other word; a target that is not `ready` draws as
unavailable. Choosing a ready target sends the menu's captured selection.

```json manifest=examples/plugins/send-target/manifest.json
{
  "schemaVersion": 1,
  "id": "example.send-target",
  "name": "Selection Receiver",
  "version": "1.0.0",
  "author": "Flea contributors",
  "license": "MIT",
  "description": "A local protocol fixture that acknowledges the selected paths.",
  "kinds": ["send-target"],
  "sendTargets": {"label": "Selection Receiver", "glyph": "network", "refreshSec": 10},
  "access": ["selection"],
  "entryPoints": {"run": "plugin.py"}
}
```

```json exchange=send-target
{"op":"targets"}
{"op":"targets","targets":[{"id":"receiver","label":"Local Receiver","state":"ready"},{"id":"offline","label":"Offline Receiver","state":"offline"}]}
{"op":"send","target":"receiver","paths":["/home/me/notes.txt"]}
{"op":"progress","done":1,"total":1,"message":"Receiving selection."}
{"op":"done","message":"Selection received."}
```

#### menu-action

A row appears in the file menu, the background menu, or both, while the selection matches `when`. Choosing it sends
`run` with the action id, the captured selection and the plugin's clamped `settings`. `folder`, the folder the menu was
opened in, is sent only when the manifest declares the `folder` access. An entry's `glyph` names the row's mark: `hash`
or one of the file menu's own marks (`app-window archive archive-out check checkmark chevron-right clipboard code
columns contrast copy copy-x download drive eject eye eye-off file file-plus file-text folder folder-open folder-plus
globe image images info key lock minus network play plus printer rename scissors server shelf sliders sort star symlink
tag terminal trash type undo bot x zap cloud-upload laptop smartphone`). It defaults to `code`, and any other name draws
no mark.
This example asks for the expected checksum in Flea's input card, described above.

```json manifest=examples/plugins/checksums/manifest.json
{
  "schemaVersion": 1,
  "id": "example.checksums",
  "name": "Checksums",
  "version": "1.0.0",
  "author": "Flea contributors",
  "license": "MIT",
  "description": "Calculate SHA-256 for the selected files.",
  "kinds": ["menu-action"],
  "menuActions": [
    {
      "id": "sha256",
      "label": "Verify Checksum…",
      "glyph": "hash",
      "when": {"minItems": 1, "maxItems": 512, "folders": false},
      "input": {"key": "checksum", "title": "Verify Checksum", "label": "Checksum", "submit": "Verify",
                "placeholder": "", "maxChars": 128}
    }
  ],
  "access": ["selection"],
  "entryPoints": {"run": "plugin.py"}
}
```

```json exchange=checksums
{"op":"run","action":"sha256","paths":["/home/me/notes.txt"],"settings":{},"input":{"checksum":"9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"}}
{"op":"progress","done":1,"total":1,"message":"Verifying checksum."}
{"op":"done","message":"Checksum matches."}
{"op":"run","action":"sha256","paths":["/home/me/notes.txt"],"settings":{},"input":{"checksum":"0000000000000000000000000000000000000000000000000000000000000000"}}
{"op":"progress","done":1,"total":1,"message":"Verifying checksum."}
{"op":"error","message":"Checksum does not match."}
```

#### rail-section

Flea asks for the section at start and every `refreshSec` seconds and draws it after the built-in rail sections, under
`railSection.title`, at most 64 rows. A `refreshSec` is rounded and clamped to 5 to 3,600; the default is 30. Each row
needs a unique nonempty `id` of at most 256 characters, a nonempty `label` and an `opens` location of at most 4,096
bytes. `opens` is an absolute folder, a `file://` URI or another `scheme://host/path` URI, with no `.` or `..`
segment. Labels longer than 256 characters are cut. `glyph` (`folder`, `server`, `drive`, `network`, `globe`, `cloud`
or `phone`, default `folder`) and `state` (`on` or `off`) are optional. A row missing `id`, `label` or `opens`, one of
those longer than 4,096 bytes or holding a NUL, or an empty or repeated `id` fails the whole answer as described under
Runtime protocol. Flea drops any other invalid row on its own and keeps the others. A `glyph` or `state` outside
those words is ignored. Activating a row opens its location in the pane. This example lists fixed folders under
`/tmp/flea-example-syncthing`, or under `FLEA_EXAMPLE_FOLDERS` when that variable is set.

```json manifest=examples/plugins/syncthing/manifest.json
{
  "schemaVersion": 1,
  "id": "example.syncthing",
  "name": "Syncthing",
  "version": "1.0.0",
  "author": "Flea contributors",
  "license": "MIT",
  "description": "Example synced folders with deterministic local fixture paths.",
  "kinds": ["rail-section"],
  "entryPoints": {"run": "bin/run"},
  "railSection": {"title": "Syncthing", "refreshSec": 5},
  "access": []
}
```

```json exchange=syncthing
{"op":"section"}
{"op":"section","rows":[{"id":"photos","label":"Photos","glyph":"folder","state":"on","opens":"/tmp/flea-example-syncthing/Photos"},{"id":"notes","label":"Notes","glyph":"folder","state":"on","opens":"/tmp/flea-example-syncthing/Notes"},{"id":"music","label":"Music","glyph":"folder","state":"off","opens":"/tmp/flea-example-syncthing/Music"}]}
```

#### place

A place adds one rail row labelled `place.label`, with a `glyph` from the rail-section list. With `place.path` set to
an absolute folder, the row opens that local folder and the plugin is never asked to list it. A `path` that is not a
valid location removes the row. Without `path`, the row opens `plugin://<id>/`, a listing the plugin owns.

Flea names items inside the place by paths relative to the place root, always starting with `/`. Entry names cannot be
empty, `.` or `..`, longer than 255 bytes, or contain `/`, `\`, a control character or a bidirectional control. `kind` is `dir` or `file`;
`size` is whole bytes and `mtime` is whole Unix seconds or an RFC 3339 time. Rename, copy, move and delete use Flea's
own cards and confirmations, and only the captured rows are sent. Rename sends the item as both `path` and `paths`.
Copy and move send `dest`, `sourceLocal`, `destinationLocal` and `overwrite`. When one side is a local folder, its
paths are absolute host paths and the matching `...Local` flag is true. A copy between two different plugin places is
refused. Flea always sends `overwrite` as false. This example keeps its listing in memory and accepts transfers only
inside its own place.

```json manifest=examples/plugins/virtual-place/manifest.json
{
  "schemaVersion": 1,
  "id": "example.virtual-place",
  "name": "Example Files",
  "version": "1.0.0",
  "author": "Flea contributors",
  "license": "MIT",
  "description": "An in-memory place for exercising listings and routed operations.",
  "kinds": ["place"],
  "entryPoints": {"run": "bin/run"},
  "place": {"label": "Example Files", "glyph": "folder"},
  "access": []
}
```

```json exchange=virtual-place
{"op":"list","path":"/"}
{"op":"list","entries":[{"name":"Photos","kind":"dir","size":0,"mtime":"2026-10-17T10:00:00Z"},{"name":"notes.txt","kind":"file","size":99,"mtime":"2026-10-17T10:00:00Z"}]}
{"op":"rename","paths":["/notes.txt"],"path":"/notes.txt","to":"draft.txt","overwrite":false}
{"op":"done"}
{"op":"copy","paths":["/draft.txt"],"dest":"/Photos","destinationLocal":false,"sourceLocal":false,"overwrite":false}
{"op":"done"}
{"op":"delete","paths":["/Photos/lake.png"]}
{"op":"done"}
{"op":"list","path":"/Photos"}
{"op":"list","entries":[{"name":"draft.txt","kind":"file","size":99,"mtime":"2026-10-17T10:00:00Z"}]}
```

#### previewer

A previewer answers only for a file that no built-in preview draws, in the preview column and in Quick Look. Flea
matches `mime` against the type it derives from the file name, not the file's content. It does not run in the lasting
runtime. Each request starts the executable once inside the thumbnail sandbox: no network, the plugin folder
read-only, the one selected file read-only at `/in/file`, an empty writable `/out`, and nothing else from the user's
files. The sandbox needs `bwrap` and `prlimit` installed. The executable receives `hello` and must answer `ready`
within five seconds, then receives one `preview` request. The process gets 10 seconds of CPU, 512 MiB of address space
and 32 MiB per written file, and the answer must arrive within 10 seconds. `output` is `text` or `png`. Text is shown
as plain lines in the text preview's own view, at most 1,000 lines, with control characters other than tab and newline
removed. A picture is written to `/out/preview.png`, answered as `{"op":"preview","png":"/out/preview.png"}` with no
`text`, and must be a PNG of at most 8,192 pixels a side. In the exchange below, `/in/file` holds a torrent of two
files.

```json manifest=examples/plugins/torrent-preview/manifest.json
{
  "schemaVersion": 1,
  "id": "example.torrent-preview",
  "name": "Torrent Preview",
  "version": "1.0.0",
  "author": "Flea",
  "license": "MIT",
  "description": "Shows a torrent file's name, size and files as plain text.",
  "kinds": ["previewer"],
  "entryPoints": {"run": "preview.py"},
  "previewer": {"mime": ["application/x-bittorrent"], "output": "text"},
  "access": ["selection"]
}
```

```json exchange=torrent-preview
{"op":"preview","path":"/in/file","outDir":"/out","width":900,"height":600}
{"op":"preview","text":"photos\n2 files, 511 bytes\n\nlake.png  412 bytes\nnotes.txt  99 bytes"}
```

#### shelf

A shelf plugin names an Omarchy bar plugin in `shelf.omarchyPlugin`: ASCII letters, digits, `.`, `_` and `-`, not
starting with `-`. Enabling or disabling the Flea plugin runs `omarchy plugin enable` or `omarchy plugin disable` with
that name, with a five second limit. If that command fails, the switch still changes and the plugin is listed as
failed with `plugins.invalid.start`. Removing an enabled shelf plugin runs the disable command first, and remove
refuses when it fails. Settings shows the plugin's generated page.

The Add to Shelf menu row appears while any shelf plugin is on and ready, the built-in Shelf or one you added. Its
files go to the first of them in the plugin list, which puts the built-in Shelf first when both are on. A shelf plugin
receives `{"op":"shelf","action":"add","paths":[...]}` from that row. It answers `done`, or `error` with a message that
Flea shows as the notice. There is no repository example for this kind, because enabling it changes the user's Omarchy
bar.

```json manifest=shelf.json
{"schemaVersion": 1,"id":"example.pile","name":"Pile","version":"1.0.0","author":"Me","license":"MIT",
"description":"Switches a bar pile.","kinds":["shelf"],"entryPoints":{"run":"bin/run"},
"shelf":{"omarchyPlugin":"pile"},"access":[]}
```

### Run an action from outside

`flea --action <id>` runs one action in a running Flea window, the same way a key or a menu row runs it.
Scripts, launchers and Omarchy shell plugins use it. It sends one request over the window's existing IPC socket,
and it never starts Flea: with no window running it fails with status `5`.

```sh action
flea --actions
flea --action viewGrid
flea --action example.checksums:sha256
```

`flea --actions` prints one id per line: Flea's built-in actions, then the menu actions of every enabled plugin.
A built-in id is an `action` from Flea's key table, `keys.toml`, such as `viewGrid`, `selectAll` or `newFolder`.
A plugin id is `<plugin id>:<action id>`, so the checksums example's action is `example.checksums:sha256`.
A plugin's ids are listed and accepted only while the plugin is installed, enabled and not failed.

The action runs in the Flea window Hyprland focused most recently. To choose a window, add `--window <pid>` with
the window's process id as `hyprctl clients` reports it. Without Hyprland, or when no Flea window has been focused,
the newest window runs it.

A built-in runs as if its key were pressed in the window's current state. A listing action needs the listing to
hold the keys, so it is refused while a menu, card, field or rename editor has them.
A plugin action runs as if it were chosen from the file menu at the cursor, or from the selection when there is one.
An action that declares only the background menu runs from the folder's background menu.
It goes through the same `when` checks, backend validation and text input card as a click on the row.

| Exit status | Meaning |
| --- | --- |
| `0` | The window ran the action. |
| `2` | The command line is not one of the two forms above. |
| `3` | No action has that id. |
| `4` | The window did not run the action: it is not available in the window's current state, the window is an older Flea, or the window did not answer. stderr says which. |
| `5` | No Flea window is running, or none has the `--window` process id. |

Every failure prints one line on stderr and nothing on stdout. A script can test the status:

```sh action
if ! flea --action example.checksums:sha256 --window "$flea_pid"; then
  echo "the checksum action did not run" >&2
fi
```

## 5. Runtime protocol

The versioned contract is [docs/plugin-protocol.json](docs/plugin-protocol.json).
Stdin and stdout carry UTF-8 JSON objects, one complete object per newline. Flush each output line.
The executable runs directly from its plugin folder; the host does not invoke a shell around its entry path.
Keep stdout for protocol messages and diagnostics on stderr.
The process runs in its own process group with `LC_ALL=C`, at most 512 MiB of address space and 256 open files.
Runtimes that reserve a large virtual address space at start can fail under that limit.

Flea's first message is:

```json
{"op":"hello","protocolVersion":1,"flea":"0.4.0","language":"en","decimal":".","settings":{},"kinds":["menu-action"]}
```

`flea` is the package version. `language` and `decimal` describe the selected locale.
`settings` contains clamped values for the manifest's schema. The five seconds for the answer count from the start
of the process:

```json
{"op":"ready","protocolVersion":1}
```

A ready response without `protocolVersion` is accepted for compatibility with the original handshake.
If supplied, the version must be 1. A malformed first response, unsupported version, early EOF or timeout fails startup.

After readiness, each plugin handles one request at a time. A second request that arrives while one is running is
not queued: it is refused at once with `plugins.error.busy`. One process is reused until it is idle for 30 seconds,
is disabled or removed, its settings change, or it fails. After an idle stop or a settings change, the next request
starts it again. Stay running between requests: a process that exits on its own is a failure.
A request has a 30-second deadline.
Each message is limited to 65,536 bytes, a selection to 512 paths and a result to 256 rows.
An answer's `op`, `message` and `png`, and each field a kind's rows need, are at most 4,096 bytes with no NUL.
A previewer's `text` fails only over the message limit and is shown as the previewer section describes. Row identities (`id`, or `name` for entries) are
nonempty and unique within one answer. Progress `done` and `total` are whole numbers with `done` at most `total`.
Progress is limited to 128 messages and 1,048,576 aggregate bytes per request.

A timeout, an early exit, a malformed or unexpected answer, or an answer over these bounds is a failure.
Flea stops the process, marks the plugin failed with `plugins.invalid.start`, and its contribution rows disappear.
The plugin stays failed until it is disabled and enabled again, or updated.

| Request | Declared kind | Final response |
| --- | --- | --- |
| `targets` | `send-target` | `targets` with a `targets` array. |
| `send` | `send-target` | `done` or `error`. |
| `run` | `menu-action` | `done` or `error`. |
| `section` | `rail-section` | `section` with a `rows` array. |
| `list` | `place` | `list` with an `entries` array. |
| `copy`, `move`, `rename`, `delete` | `place` | `done` or `error`. |
| `preview` | `previewer` | `preview` with plain `text` or the fixed `/out/preview.png` output. |
| `shelf` | `shelf` | `done` or `error`. Sent only as described under shelf above. |
| `settings` | Any kind | `done` or `error`. |

Operations ending in `done` may first send progress:

```json
{"op":"progress","done":1,"total":3,"message":"notes.txt"}
```

Finish with one response:

```json
{"op":"done"}
```

Or report a plain-text error:

```json
{"op":"error","message":"The destination is unavailable. Reconnect it and retry."}
```

Flea parses these values as data. It does not execute returned strings or render them as HTML or other markup.
An `error` answer is an ordinary operation result. It does not mark the plugin failed, and Flea shows its message.
When Flea stops a process, it writes `{"op":"cancel"}` and kills the whole process group at once. Do not rely on
time to clean up after cancel. Stderr is discarded; capture development logs yourself.

Settings > Plugins keeps a failed plugin's row and page, with `failed to start` in error ink. It does not show the
reason; `failure` and `detail` in the owner process's listing carry it.
Runtime failures belong to the application's owner process. A fresh CLI listing can show disk validation failures,
but it does not read another process's remembered runtime failure.

## 6. Lifecycle

### Add, validate and enable

```sh
flea plugin validate /absolute/path/to/plugin
flea plugin add /absolute/path/to/plugin
flea plugin add 'https://example.org/author/plugin.git'
flea plugin enable example.hello
```

The URL above is a placeholder. Substitute an actual Git repository with `manifest.json` at its root.
Add classifies strings containing `://` or starting with `git@` as Git sources.
Local sources must be existing absolute folders; add also expands a literal `~/` prefix.
For validate, pass an absolute folder path. It does not clone Git or expand a literal `~/`.
Neither command builds source code or installs dependencies. Ship an executable or an executable script.

Validate checks the manifest and executable path. It never runs the executable and checks no contribution behavior.
Add stages a copy or shallow clone, checks the manifest and free id, then renames it into place.
Add does not check executable readiness or even require the executable to exist yet.
Run validate before add, then enable separately.
Newly added plugins are disabled. A fresh add clears saved state left by a previous copy.

A folder add skips every dot entry, including `.git`, and copies regular files and folders.
Symlinks and special entries are not copied. Do not depend on them for packaged assets.
A Git clone keeps the symlinks in its repository.
Files retain executable bits but lose special permission bits.
The copied tree may contain at most 4,096 counted entries and 64 MiB of file data, with at most 16 nested folder levels.
A Git clone is measured with the same bounds, including dot entries except its top-level `.git`.
A folder source is recorded in `plugins/.sources/<id>` outside the plugin.

### Update

```sh
flea plugin update example.hello
```

Update works only for a plugin added from Git.
It executes `git pull --ff-only --quiet` in the installed repository.
Do not edit the installed checkout if you want straightforward fast-forward updates.
Clone and pull have a 120-second deadline.
Before pulling, inspecting `.git/config` through Git has a separate ten-second deadline.
A Git error or timeout selects `plugins.error.git`. Git's own output is discarded, and the detail names the source or
origin URL. Run the same `git` command yourself to see the cause.
Update is an in-place pull. It does not use add's staging-and-rename path or repeat its full tree-size check.

Flea accepts only a plain `.git` directory with no `commondir` and a small regular config of at most 64 KiB.
The config may hold the ordinary clone keys: repository format, file mode, bare state, reflog setting,
ignorecase, precomposeunicode, symlinks, origin URL and fetch, plus branch remote and merge keys.
Extra settings, including includes, hooksPath, fsmonitor and worktree relocation, refuse update.
Git runs with system and global config disabled, hooks and fsmonitor disabled, external helper protocol disabled,
terminal prompts disabled, and repository-selection environment variables removed.
Private repositories must work noninteractively without relying on the disabled user Git config.

An enabled plugin is checked again after a successful pull and manifest reread.
A disabled plugin's executable is not run during update.

### Disable and remove

```sh
flea plugin disable example.hello
flea plugin remove example.hello
```

Disable clears a remembered startup failure in that process.
Remove deletes only the installed plugin folder, its source record and saved plugin state.
For an enabled shelf plugin, it first runs the Omarchy disable command described under shelf.
It does not delete the original source folder or run an uninstall hook.
GPUI asks for confirmation before Remove; the CLI removes immediately.

### Built-ins and restarts

Shelf, LocalSend, Tailscale and Dropbox ship inside Flea and update with the package.
Their reserved ids start with `flea.`. Their switches appear in Settings > Plugins.
They cannot be removed or updated with the plugin verbs.
Shelf is off by default; the other three are on by default and depend on their installed applications or accounts.
Shelf uses the `shelf.enabled` setting rather than a separate `plugins` switch.

Enable restarts the supervised session and checks readiness when no validation failure prevents it.
Updating an enabled Git plugin stops the old session before rereading and checking the new one.
Opening a contribution starts its executable on demand. A settings change reaches an active session, which then
stops; the next request starts a new process.
Disable, removal and application exit stop the owner's plugin processes.

## 7. Security model

Treat an executable plugin as code running under your own account.
It inherits Flea's environment, with `LC_ALL=C`, and can access files, network services and programs allowed to that
account. Its only confinement is the address space and open file limits under Runtime protocol.
The `access` list is disclosure. It neither restricts nor elevates OS permissions.
Ordinary plugin processes inherit the host environment and normal user permissions.
Preview requests must use the thumbnail sandbox: one read-only selected input, a private output folder and no network.
Only install code you have reviewed or trust. Ordinary plugin processes are not confined by the access list.

Flea refuses invalid ids, the reserved prefix, unsafe entry paths, missing kind blocks, invalid settings schemas,
unreadable manifests and duplicate identities.
Entry resolution prevents a plugin from naming an executable outside its canonical folder.
Folder installation bounds the copied tree. Git update checks repository configuration before invoking pull.
Remove requires an absolute direct child of the plugin store and does not traverse a top-level symlink to delete its target.
These checks protect Flea's own lifecycle operations. They do not confine the plugin process.

## 8. Testing and debugging

Use isolated XDG folders to keep development installs separate from your normal configuration:

```sh
plugin_test_root=$(mktemp -d)
printf '%s\n' 'flea-plugin-development' >"$plugin_test_root/.flea-test-sandbox"
export XDG_CONFIG_HOME="$plugin_test_root/config"
export XDG_DATA_HOME="$plugin_test_root/data"
export XDG_CACHE_HOME="$plugin_test_root/cache"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME"
flea plugin validate "$HOME/flea-plugin-hello"
flea plugin add "$HOME/flea-plugin-hello"
flea plugin enable example.hello
flea plugin list --json
flea --gui
```

Close the test GUI, then remove the installed example while those XDG variables are still exported:

```sh
flea plugin remove example.hello
```

Check the install copy, executable permissions, shebang interpreter and runtime dependencies.
Changing the development source does not change a folder installation.
Remove and re-add a folder plugin to copy new files.
Use disable then enable in the same GPUI session to inspect startup failure behavior.
Do not use a successful enable exit status alone as proof of readiness.

For a machine-readable outcome and the following failure listing, start one backend process:

```sh
flea --backend
```

Enter this line, then wait for the outcome and listing before sending quit or closing input:

```json
{"c":"plugins","op":"enable","id":1,"plugin":"example.hello"}
```

Requests run on worker threads. A separate list can race a change; use the listing with the enable request's id.
Each change answers a `pluginop` outcome followed by its `plugins` listing with that same id.
The wire also supports `set`, although `flea plugin set` is not a CLI command:

```json
{"c":"plugins","op":"set","id":3,"plugin":"example.settings","key":"timeout","value":13}
```

That value clamps to 12 for the example's range and step.
The GUI exposes the same settings through its controls.
See [docs/protocol.md](docs/protocol.md) for the backend wire, which is distinct from the executable's hello pipe.

### Refusals and fixes

In a `pluginop` outcome, `code` is the unprefixed final component; in the listing, the same value is `failure`.
The table gives its full English catalog key and the repair.
`detail` names the field, plugin, path or underlying cause where available.

| Catalog key | Fix |
| --- | --- |
| `plugins.invalid.json` | Make the manifest a valid JSON object within 64 KiB. |
| `plugins.invalid.schema_version` | Set numeric `schemaVersion` to 1. |
| `plugins.invalid.missing` | Supply the named field with the required type and required contents. |
| `plugins.invalid.id` | Use at least two lowercase dot-separated segments within 64 bytes. |
| `plugins.invalid.reserved` | Choose your own namespace outside `flea.`. |
| `plugins.invalid.kind` | Correct the kind token; an arbitrary new token cannot define a new surface. |
| `plugins.invalid.block` | Supply the block required by the declared kind. |
| `plugins.invalid.run` | Use a relative path with no NUL or `..` component for `run` and `mark`. |
| `plugins.invalid.field` | Correct the setting definition or duplicate key using the schema rules above. |
| `plugins.invalid.long` | Shorten the named text field to its character limit. |
| `plugins.invalid.start` | Startup or a request failed. Read hello and flush a complete ready line within five seconds, answer every request within 30 seconds inside the protocol bounds, and keep running between requests. `detail` names the cause. Disable and enable to clear it. |
| `plugins.invalid.exec` | Supply an executable regular file inside the plugin, with a usable interpreter and dependencies. |
| `plugins.invalid.manifest` | Repair the installed folder's readable, regular UTF-8 `manifest.json` within 64 KiB. |
| `plugins.invalid.duplicate` | Remove or rename the extra manually installed folder claiming the same id. |
| `plugins.error.not_found` | Pass add an existing absolute folder, a `~/` folder or a Git URL. |
| `plugins.error.no_manifest` | Supply a readable, regular UTF-8 root `manifest.json` within 64 KiB at the source. |
| `plugins.error.exists` | Remove the previous installation or choose another id. |
| `plugins.error.git` | Check the origin, network, noninteractive authentication, plain repository config and fast-forward history. Folder plugins cannot update. |
| `plugins.error.write` | Check store permissions, available disk space, safe file types and installation tree bounds. |
| `plugins.error.unknown` | Use an id from `flea plugin list`. |
| `plugins.error.builtin` | Update Flea itself; use the built-in's switch to disable it. |
| `plugins.error.setting` | Use a declared setting key and check that `ui.json` can be saved. |
| `plugins.error.busy` | The plugin is already handling a request, or the owner has 16 requests in flight. Wait for the answer and repeat. |
| `plugins.error.request` | Use a supported operation and the request shape in the backend protocol. |

### Repository documentation check

From a Flea source checkout, these tests check this guide's examples and marked fences against the code.
They need Python 3; the previewer case also needs `bwrap` and `prlimit`.

```sh
cargo test -p flea --test plugins_doc --test plugins_doc_kinds --test plugins_keys_doc
```

`plugins_doc` checks every marked manifest with the real validator, every documented refusal key against the English
catalog, the quick start executable and the `flea --action` lines. `plugins_doc_kinds` checks that each kind's
manifest, shelf aside, matches its example under `examples/plugins`, runs each exchange against the example's
executable and the protocol checks, and adds, enables and runs each example through Flea's plugin runtime. `plugins_keys_doc` checks the
`keys.toml` example.

Relevant implementation: `src/ui_logic_plugin_settings/{manifest,contributions,schema,pages,wire,store}.rs`,
`src/plugin_runtime/`, `src/backend/{plugins,pluginfs,pluginadd,pluginops,plugingit,pluginruntime,pluginpreview}.rs`,
`src/plugincli.rs`, `crates/flea-gui/src/g2/plugins/` and `crates/flea-gui/src/g2/plugin_settings/`.

## 9. Checklist for coding agents

1. Choose one of the six declared kinds and use its existing Flea surface.
2. Choose a unique namespaced id outside `flea.` and keep the install folder named after it.
3. Write a complete schemaVersion 1 manifest, staying within its field and byte limits.
4. Ship a regular executable with a valid interpreter. Add does not build it.
5. Test hello and a flushed ready response. Stay running between requests and exit when stdin closes.
6. Declare actual access honestly. Assume normal user permissions, not an access-scope sandbox.
7. Validate before add. Enable separately and inspect the same process's failure listing.
8. Test every setting's default, saved value and invalid-value clamp.
9. Inspect the actual GPUI installed row and settings page in isolated XDG directories.
10. Test disable and removal. Use Git update only for Git installations.
11. Keep diagnostics out of stdout and capture your own development logs.
12. Exercise the real contribution, its failure paths, disable and removal. A valid manifest and ready handshake alone are not surface proof.
