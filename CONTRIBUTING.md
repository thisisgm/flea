# Contributing to Flea

Bug reports, fixes and features are all welcome.

## Reporting a bug

Try the [latest release](https://github.com/thisisgm/flea/releases/latest) first, since the bug may
already be fixed. Then open an issue with the bug report form, one bug per issue. The reports that get
fixed fastest have

- exact steps from a fresh Flea window, with the shell lines that create any files the bug needs,
- a screenshot or a screen recording (Print takes a screenshot, Alt+Print records the screen),
- the output of `flea --version`, `omarchy-version` and `omarchy-theme-current`,
- for a crash, the output of `coredumpctl info` for it.

Report a security problem through the [security policy](SECURITY.md), never in a public issue.

## Asking for a feature

Open an issue with the feature request form and describe what you want to do, not only the control
you picture. A mockup or a capture of another app doing it helps. Flea keeps its defaults stable, so
a new option usually ships off and a fresh install looks like the last release.

## Pull requests

- One change per pull request, rebased on the latest `main`, with its issue linked when there is one.
- [Build Flea](README.md#build) and run the headless suites before you open it:

  ```bash
  ./tests/run-all.sh
  ```

- Show proof. A screenshot or recording before and after for any change you can see, and for a fix,
  the steps that failed before and pass now.
- Keep the diff to the change. Formatting or refactoring elsewhere goes in its own pull request.
- A change to how Flea looks follows its existing design. One that needs new design may wait for a
  later release.

The optional local hooks are described in [development checks](docs/development-checks.md), and
[the protocol guide](docs/protocol.md) documents the backend.

When a pull request lands, your commits keep your name. Fixes on top arrive as separate commits, and
work that is reimplemented credits you with a `Co-authored-by` line.
