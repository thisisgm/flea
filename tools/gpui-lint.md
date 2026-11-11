# Native GUI source gate

Run `tools/flea-gpui-lint` from a Git checkout with Python 3.11 or newer and the locked Cargo dependencies available.
It reads tracked Rust files in native workspace packages and `cargo metadata --locked --format-version 1`.
In an archived CI export without Git, it checks every exported native source and manifest.
Comments and Rust string, raw string, byte string, C string and character literals are opaque tokens.
It follows imported aliases and local helper calls; only explicit thread or background-executor closures are background boundaries. Rust builds a package's library and each binary as separate crates, but a path cannot tell a binary's module from the library's, so the lint treats every src/ file of one package as one target; this over-approximates reach and can only add findings. A call into another package resolves only through its library, and an integration test, example, bench or build script is never reached from another target.
Imports and local callable aliases follow lexical scopes, including parent-module glob imports. A background boundary must belong to this spawn's receiver and directly supplied closure or future, not another expression in the statement.
Qualified standard-library joins and blocking channel receives normalize their receiver types, generic arguments and imported aliases before classification; Path::join remains nonblocking.
Method calls reach source declarations with a receiver parameter; unrelated free functions with the same name do not become method callees.
A method called on a field chain (`self.shared.claim()`, or a local bound by `let air = &mut shell.menu.providers.airdrop;`) resolves through the declared field types, so a method name several types share still reaches the field's own type; a field name every struct declares with one type stands in for an unknown root, and a wrapped type such as `Option<T>` is not unwrapped. A type that declares no method of that name under its own name (a wrapper such as `Arc<T>`, a renamed import, a method written in an `impl super::T` block) leaves the call resolved by the method name alone, as on an untyped receiver, so typing a receiver never narrows reach to nothing. A method exception keeps naming the method as written. Colour setters require a call expression, so a boolean named `background` followed by an `if` block is not a setter.
In registered hot-surface files, every integer token spelled as 0x plus six or eight hex digits, with Rust underscores ignored, is a finding, even in an unused binding or nested write. Constructor arguments are checked for literal tokens directly. No literal provenance is inferred through variables or control flow.

`tools/gpui-lint.json` is the single registry and exception file:

- `fork` records the exact public Git URL and full revision. Manifests, Cargo.lock and resolved packages must agree.
- `ui_callbacks` names full Rust function symbols. Paint and prepaint methods are always checked.
- `blocking_helpers` names additional full symbols that must never run on the UI thread.
- `blocking_exceptions` requires an exact `caller`, exact called `symbol` and nonempty `reason`. Wildcards fail.
- `color_modules` requires an exact `module`, `kind` of `palette` or `content`, and a `reason`. Drawing and callback modules cannot be exempted.
- `non_color_hex_constants` permits a genuine non-colour mask through an exact module-level constant `symbol`, exact hex `literal` and nonempty `reason`. It exempts that declaration only; constructor literal arguments remain forbidden. The product currently needs no entries.
- `hot_surfaces` records each surface's file, custom `element` and fixed `row_height` constant. An optional `row_height_path` names the sibling file that declares the constant; the Element file must still use it. A pending surface must be registered when its file appears.

Run `tests/gpui-lint.sh` and `tests/budget.sh` for mutants and controls.
The lint suite includes full-CLI review regressions in `tests/gpui_lint_regressions.py`, with a metadata-only Cargo fixture.
The lint suite also checks the real source tree; `tests/gpui-lint.sh --tests-only` runs just its fixture controls.
`tests/gpui-gates.sh` additionally runs real Clippy, formatting and architecture gates and rejects an injected GUI warning in a private source copy.
All test writes and cleanup stay within marked temporary roots.

This source gate does not prove zero warmed-paint allocations, visible-range request bounds or atomic swaps.
Element tests and runtime probes own those checks.
It is a lexical guard backed by review and runtime frame/UI-thread gates, not a Rust dataflow or soundness proof.
The native list scene tests check the default root canvas and application theme roles. Palette loading runs on the startup worker before the application starts; `flea_gui::window::open` is the UI setup callback the application runs.
The root license policy limits additional licenses to exact package/version exceptions.
Run `tests/license-policy.sh` in the license lane with `cargo-deny` installed; it requires a version bump to lose its exception.
Audit the full graph with `cargo deny --locked --workspace check`:
Cargo's default member is the core crate, so the command without `--workspace` does not audit the GUI.
