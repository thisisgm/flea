"""Exercise the visual Trash opener with asynchronous rail readers."""
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
            if [ -e "$model/opened" ]; then printf '{"opened":true,"total":1,"busy":false}\n';
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
    def run_opener(self, scenario):
        source = SOURCE.read_text()
        opener = re.search(r'^open_trash\(\) \{\n.*?^\}', source, re.M | re.S)
        self.assertIsNotNone(opener, 'the real visual Trash opener must exist')
        with tempfile.TemporaryDirectory(prefix='flea-trash-navigation.') as root:
            model = Path(root)
            (model / '.flea-test-sandbox').touch()
            result = subprocess.run(
                ['bash'], input=MOCKS + '\n' + opener.group() + '\nopen_trash\n',
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


if __name__ == '__main__':
    unittest.main()
