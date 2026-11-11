"""Exercise visual state navigation with asynchronous readers and real shell cases."""
from pathlib import Path
import os
import re
import subprocess
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[2]
SOURCE = REPO / 'ci/visual/lane/trash.sh'
MOCKS = r'''
set -uo pipefail
W=/fixture
case_notes=/dev/null
case_name=trash-navigation
launch() { printf '0\n' > "$model/cursor"; }
state_failure() { printf '%s\n' "$*" >&2; return 1; }
sleep() { :; }
key() {
    local cursor next
    cursor=$(cat "$model/cursor")
    case "$*" in
        '-k Tab') ;;
        g) printf '0\n' > "$model/cursor" ;;
        j|k)
            if [ "$1" = j ]; then next=$((cursor + 1)); else next=$((cursor - 1)); fi
            [ "$next" -le 4 ] || next=4
            if [ "$scenario" = delayed ]; then
                printf '%s\n' "$next" > "$model/pending"
            else
                printf '%s\n' "$next" > "$model/cursor"
            fi
            ;;
        '-k Return')
            printf '%s\n' "$cursor" >> "$model/activations"
            [ "$cursor" != 2 ] || touch "$model/opened"
            ;;
        *) state_failure "unexpected key $*"; return 1 ;;
    esac
}
ipc() {
    local cursor
    cursor=$(cat "$model/cursor")
    case "$1" in
        focusView) printf 'rail\n' ;;
        railCursor)
            if [ -f "$model/pending" ]; then
                mv "$model/pending" "$model/cursor"
            fi
            printf '%s\n' "$cursor"
            ;;
        railEntries)
            if [ "$scenario" = missing ]; then printf '[]\n';
            elif [ "$scenario" = shifted ] && [ -e "$model/entries-read" ]; then
                printf '[{"kind":"home"},{"kind":"recent"},{"kind":"share"},{"kind":"trash"},{"kind":"disk"}]\n'
            else
                touch "$model/entries-read"
                printf '[{"kind":"home"},{"kind":"recent"},{"kind":"trash"},{"kind":"share"},{"kind":"disk"}]\n'
            fi
            ;;
        railLabel)
            printf '%s\n' "$2" >> "$model/label-reads"
            printf '\n'
            ;;
        trashState)
            local total=1
            [ "$scenario" != two ] || total=2
            if [ -e "$model/opened" ]; then printf '{"opened":true,"total":%s,"busy":false}\n' "$total";
            else printf '{"opened":false,"total":0,"busy":false}\n'; fi
            ;;
        *) state_failure "unexpected reader $*"; return 1 ;;
    esac
}
wait_ipc() {
    local i
    for ((i=0; i<3; i++)); do
        [ "$(ipc "$1")" != "$2" ] || return 0
    done
    state_failure "reader $1 never reached $2"
}
wait_ipc_has() {
    [[ "$(ipc "$1")" == *"$2"* ]] || state_failure "reader $1 lacks $2"
}
'''


class TrashNavigation(unittest.TestCase):
    def run_opener(self, scenario, expected=1):
        source = SOURCE.read_text()
        opener = re.search(r'^open_trash\(\) \{\n.*?^\}', source, re.M | re.S)
        self.assertIsNotNone(opener, 'the real visual Trash opener must exist')
        with tempfile.TemporaryDirectory(prefix='flea-trash-navigation.') as root:
            model = Path(root)
            (model / '.flea-test-sandbox').touch()
            result = subprocess.run(
                ['bash'], input=MOCKS + '\n' + opener.group() + f'\nopen_trash {expected}\n',
                text=True, capture_output=True,
                env={**os.environ, 'model': root, 'scenario': scenario}, timeout=10,
            )
            activated = (model / 'activations').read_text() if (model / 'activations').exists() else ''
            opened = (model / 'opened').exists()
            self.label_reads = (model / 'label-reads').exists()
        return result, activated, opened

    def test_opens_trash_once(self):
        result, activated, opened = self.run_opener('ready')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(activated, '2\n')
        self.assertTrue(opened)

    def test_expected_total_is_required(self):
        result, activated, opened = self.run_opener('ready', expected=2)
        self.assertNotEqual(result.returncode, 0, 'one item cannot satisfy a two-row case')
        self.assertIn('reader trashState lacks "total":2,', result.stderr)

    def test_opener_accepts_the_keyboard_fixture_total(self):
        result, activated, opened = self.run_opener('two', expected=2)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(activated, '2\n')
        self.assertTrue(opened)

    def test_navigation_never_reads_painted_labels(self):
        result, activated, opened = self.run_opener('ready')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(self.label_reads, 'a blank painted label must not steer the cursor')

    def test_waits_for_each_cursor_move(self):
        result, activated, opened = self.run_opener('delayed')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(activated, '2\n')
        self.assertTrue(opened)

    def test_missing_trash_never_activates_another_row(self):
        result, activated, opened = self.run_opener('missing')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('rail has no Trash entry', result.stderr)
        self.assertEqual(activated, '')
        self.assertFalse(opened)

    def test_reordered_rail_never_activates_another_row(self):
        result, activated, opened = self.run_opener('shifted')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('rail row 2 is no longer Trash', result.stderr)
        self.assertEqual(activated, '')
        self.assertFalse(opened)


PAIR_MOCKS = r'''
set -uo pipefail
VISUAL_FRONTEND=gpui W=/fixture STAMP='2025-06-15 10:00:00'
printf '0\n' > "$model/cursor"
state_failure() { printf '%s\n' "$*" >&2; return 1; }
launch() { printf 'launch %s\n' "$LAUNCH_STATE" >> "$model/trace"; }
size_window() { :; }
sleep() { :; }
park_pointer() { :; }
mv() {
    [ -s "$XDG_DATA_HOME/Trash/info/keyboard-draft.txt.trashinfo" ] || return 1
    command mv "$@" || return 1
    printf 'fixture-published\n' >> "$model/trace"
}
fxpolish_tab_to() { printf '%s\n' "$1" > "$model/focus"; }
open_trash() {
    printf 'open %s\n' "$1" >> "$model/trace"
    [ "$1" = 2 ] && [ -s "$XDG_DATA_HOME/Trash/files/keyboard-draft.txt" ] &&
        grep -qx 'DeletionDate=2025-06-13T09:30:00' "$XDG_DATA_HOME/Trash/info/keyboard-draft.txt.trashinfo"
}
key() {
    printf 'key %s\n' "$*" >> "$model/trace"
    case "$*" in
        t|g) printf '0\n' > "$model/cursor" ;;
        '-k Down') [ "$scenario" = frozen ] || printf '1\n' > "$model/cursor" ;;
        *) return 1 ;;
    esac
    return 0
}
ipc() {
    case "$1" in
        tabCount) echo 2 ;;
        tabIndex) echo 1 ;;
        tabBarVisible) echo true ;;
        tabCentre)
            [ "$scenario" != unpainted ] || return 0
            if [ "$2" = 0 ]; then echo '294 41'; else echo '490 41'; fi ;;
        focusStop) cat "$model/focus" ;;
        trashState)
            local path=$W/keyboard-draft.txt
            [ "$scenario" != wrongrow ] || path=$W/other.txt
            printf '{"opened":true,"total":2,"busy":false,"cursor":%s,"rows":[{"original":"/fixture/old-draft.txt"},{"original":"%s"}]}\n' "$(cat "$model/cursor")" "$path" ;;
        *) return 1 ;;
    esac
}
wait_ipc() { [ "$(ipc "$1")" = "$2" ]; }
wait_ipc_has() { [[ "$(ipc "$1")" == *"$2"* ]]; }
hover_rest_shot() { printf 'point %s\n' "$1" >> "$model/trace"; }
snap() { printf 'snap %s\n' "$(ipc trashState)" >> "$model/trace"; }
'''


class VisualStatePairs(unittest.TestCase):
    def run_case(self, name, scenario='ready', source=None):
        with tempfile.TemporaryDirectory(prefix='flea-visual-pairs.') as root:
            model = Path(root)
            (model / '.flea-test-sandbox').touch()
            result = subprocess.run(
                ['bash'], input=f'. "{source or REPO / "ci/visual/lane/cases.sh"}"\n' + PAIR_MOCKS + f'\n{name}\n',
                text=True, capture_output=True, timeout=10,
                env={**os.environ, 'model': root, 'scenario': scenario,
                     'HOME': root, 'XDG_DATA_HOME': str(model / 'data')},
            )
            trace = (model / 'trace').read_text() if (model / 'trace').exists() else ''
        return result, trace

    def test_tab_hover_targets_the_close_slot(self):
        result, trace = self.run_case('case_tab_hover')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('point 380 41\n', trace)
        self.assertIn('launch ,"highlightHover":true\n', trace)

    def test_tab_rest_targets_the_body(self):
        result, trace = self.run_case('case_tab_hover_off')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('point 294 41\n', trace)
        self.assertIn('launch ,"highlightHover":false\n', trace)

    def test_unpainted_tab_never_captures(self):
        result, trace = self.run_case('case_tab_hover', 'unpainted')
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('point ', trace)

    def test_trash_keyboard_moves_to_its_second_row(self):
        result, trace = self.run_case('case_trash_cursor_keyboard')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('open 2\n', trace)
        self.assertIn('key -k Down\n', trace)
        self.assertIn('snap {"opened":true,"total":2,"busy":false,"cursor":1,', trace)

    def test_trash_fixture_is_complete_before_gvfs_observes_it(self):
        result, trace = self.run_case('case_trash_cursor_keyboard')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertLess(trace.index('fixture-published\n'), trace.index('open 2\n'))

    def test_ignored_keyboard_move_never_captures(self):
        result, trace = self.run_case('case_trash_cursor_keyboard', 'frozen')
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('snap ', trace)

    def test_wrong_keyboard_row_never_captures(self):
        result, trace = self.run_case('case_trash_cursor_keyboard', 'wrongrow')
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('snap ', trace)


if __name__ == '__main__':
    unittest.main()
