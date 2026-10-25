# Suite inventory fixtures

`suite-evidence.txt` preserves CI evidence patterns and floors. `layouts/` holds
only layouts for the current test tree. Calibration uses its headless list; a
bare `default` line inherits its default layout. Dedicated protocol and build
lanes are separate checks. Scheduled scripts must exist and be executable.

`batches/trees.tsv` records the tree that owns each historical CI layout. These
layouts retain their batch suites and are checked separately with `--layout-only
--tree ARCHIVED_TREE --layout FILE`. `default.runner` is the stage tree's frozen
runner for the cross-tree regression control. Historical layouts never inherit
the current worktree's registered suites.

In flea-ci, the proposed runner adapter stages the selected layout as
`/in/suite-inventory.layout` and its evidence as `/in/suite-evidence.txt`, both in
the verified worker receipt. The suite checks that selected layout against the
tree it is running on before its fixture tests. `FLEA_CI_INPUT_DIR` relocates the
input mount for tests; `FLEA_CI_LAYOUT` and `FLEA_CI_EVIDENCE` explicitly override
the two files. A missing staged input fails instead of using a fixture. The
standalone suite broker has no selected layout and runs the fixture tests.

`check.py` uses owned temporary copies for thirteen independently rejected mutants,
each repaired into a passing twin. It also checks inherited layouts, replacement
evidence, the two obsolete mechanism retirements and the g06 projection seam.
Script mutants change only private copies and restore a passing executable twin.
Parity links in the dispositions table include suite IDs and directly named
QML/JS case IDs from g06. The JS wrapper also links every pure JS case. An explicit
`none:` reason records labels for which this tree has no independent suite.
The suite reads repository fixtures and never reads the external CI installation.
