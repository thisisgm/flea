# Keyboard matrix fixtures

`installer.json` preserves every literal installer row, without merging duplicate
US layouts used by different input methods. It records source hashes at Omarchy
`5978d93ae7da0a325b7493501911e3f1266cadd3` (v4.5.0-rc2). `omarchy/install/provisioning/setup-form.sh`
is that exact file, so `i50_layout_count_pinned` extracts the list without the
checkout. The count is read from the source; the test pins 52 for this commit only.
A new pin adds its commit and count to the test and regenerates this fixture.

`kbd-model-map` is the recorded systemd conversion table. The Omarchy provisioning
overrides and explicit installer XKB layout/variant fields take precedence.
Each compiled keymap and every active group receives a separate identity.

`samples.json` records libxkbcommon events on US, German, French, Russian, Arabic
and Hebrew maps, including raw, effective and consumed modifiers. These are
analysis fixtures, not native GUI or terminal captures. Tests require real XKB
data and libxkbcommon. Missing libraries, keymaps or variants fail explicitly.
`baseline.keys.toml` is the exact v0.3.12 table, including the legacy repeated-key
spelling that predates the explicit sequence metadata. Both formats expand into
the same strokes before normalization.

Run the registered `i18n-quality` suite through the host broker. This unit does
not own its registration. The suite is supplied by I50-02 after this base.

The generated matrix contains every canonical row, including suppressed actions,
shared listing and rail rows, all named presets (including TUI `yazi`), explicit
TUI overrides, digit rows, repeated-key sequences and prefixed TUI sequences.
Collision witnesses name physical key and held-modifier paths in overlapping
frontend, preset and context scopes. Shipped overrides are retained as findings.

Punctuation follows the approved order in `i18n/keyboard-policy.json`: (a) the
printed character on a base or Shift level without AltGr when it conflicts with
nothing in an overlapping scope; (b) otherwise the US position; (c) non-Latin groups
always use the US position; (d) a cell whose alternatives all conflict, or whose only
remaining alternative is absent, stays a failing `punctuation-conflict` row.
No command path holds AltGr. Each cell records both alternatives and its rule.
The design linter receives equivalent physical-path normalization for candidate
and v0.3.12, including the sequence's first key and continuation context.

The current GUI model follows the pinned fork's keystroke conversion and Flea's
held-Shift restoration. Its results are diagnostic, not native input proof.
Terminal protocol limitations remain explicit per row and in the policy. Native
group transitions, compositor interception and negotiated terminal reports must
be captured by the controller before I50-10/11 close runtime behavior.
