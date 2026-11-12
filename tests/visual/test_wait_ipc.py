"""Check the visual lane's real substring wait and its caller's exit status."""
from pathlib import Path
import os
import subprocess
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[2]
CASES = REPO / 'ci/visual/lane/cases.sh'
SCRIPT = r'''
set -uo pipefail
CASES=()
S=1100x700
wait_ipc_has() { return 0; }
. "$cases"
case_name=wait-contract
case_notes=$model/notes
shots=$model/shots
sleep() { :; }
ipc() {
    local count
    count=$(cat "$model/reads")
    printf '%s\n' "$((count + 1))" > "$model/reads"
    case "$scenario" in
        ready) printf '{"opened":true}\n' ;;
        delayed)
            if ((count < 2)); then printf '{"opened":false}\n';
            else printf '{"opened":true}\n'; fi ;;
        error) printf '{"opened":true}\n'; return 1 ;;
        *) printf '{"opened":false}\n' ;;
    esac
}
caller() {
    wait_ipc_has trashState '"opened":true' || return 1
    touch "$model/captured"
}
caller
'''


class WaitIpc(unittest.TestCase):
    def run_wait(self, scenario):
        with tempfile.TemporaryDirectory(prefix='flea-wait-ipc.') as root:
            model = Path(root)
            (model / '.flea-test-sandbox').touch()
            (model / 'reads').write_text('0\n')
            result = subprocess.run(
                ['bash'], input=SCRIPT, text=True, capture_output=True, timeout=10,
                env={**os.environ, 'cases': str(CASES), 'model': root, 'scenario': scenario},
            )
            return (result, int((model / 'reads').read_text()),
                    (model / 'captured').exists(),
                    (model / 'shots/.state-failures/wait-contract').exists(),
                    (model / 'notes').read_text() if (model / 'notes').exists() else '')

    def test_ready_state_captures_once_without_failure(self):
        result, reads, captured, failed, notes = self.run_wait('ready')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(reads, 1)
        self.assertTrue(captured)
        self.assertFalse(failed)
        self.assertEqual(notes, '')

    def test_delayed_state_retries_until_the_match(self):
        result, reads, captured, failed, notes = self.run_wait('delayed')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(reads, 3)
        self.assertTrue(captured)
        self.assertFalse(failed)

    def test_timeout_fails_the_caller_before_capture(self):
        result, reads, captured, failed, notes = self.run_wait('timeout')
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertEqual(reads, 60)
        self.assertFalse(captured)
        self.assertTrue(failed)
        self.assertIn('wait trashState has="opened":true got={"opened":false} reader_status=0', notes)

    def test_failed_reader_cannot_satisfy_the_wait(self):
        result, reads, captured, failed, notes = self.run_wait('error')
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertEqual(reads, 60)
        self.assertFalse(captured)
        self.assertTrue(failed)
        self.assertIn('got={"opened":true} reader_status=1', notes)


if __name__ == '__main__':
    unittest.main()
