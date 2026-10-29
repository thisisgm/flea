"""Regression checks for terminal evidence and lifecycle boundaries."""
from contextlib import ExitStack, redirect_stdout, redirect_stderr
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from . import adapters, cells, driver, dynamic, fake, media, native, receipts, reconcile, references
from .evidence import Blocked, Commands, MARKER, digest
from .geometry import CELL_HEIGHT, ORIGIN
from .review_support import media_transcript
from . import test_review_r2

PALETTE = {'fg': '#ffffff', 'bg': '#000000'}


class RegressionTests(unittest.TestCase):
    def setUp(self):
        parent = Path(os.environ.get('FLEA_BATTERY_TEST_ROOT', Path(__file__).resolve().parents[2] / '.superpowers/t12a'))
        parent.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix='regressions.', dir=parent))
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        (self.root / MARKER).write_text('owned battery regressions\n')

    def test_display_accepts_help_that_exits_one_like_hyprctl_0562(self):
        # Sample input: hyprctl --help on Hyprland 0.56.2 prints the command list, including output, and exits 1.
        monitors = [b'[]', json.dumps([{'name': 'FLEA-id', 'scale': 1, 'width': 2560, 'height': 1440, 'id': 7}]).encode()]
        def answer(argv, **_):
            function = argv[5]
            code, stdout = {'hypr_query_help': (1, b'commands: monitors clients output version'),
                            'hypr_query_output_help': (1, b'output create remove'),
                            'hypr_query_option': (0, b'{"int": 0}')}.get(function, (0, b''))
            if function == 'hypr_query_monitors':
                stdout = monitors.pop(0) if len(monitors) > 1 else monitors[0]
            return subprocess.CompletedProcess(argv, code, stdout, b'')
        commands = Commands()
        with patch('tui_battery.evidence.subprocess.run', side_effect=answer):
            with native.Display(commands, 'id') as display:
                self.assertEqual(display.monitor['id'], 7)
        self.assertEqual([r['argv'][5] for r in commands.records][-1], 'hypr_output_remove')

    def test_nonstring_font_is_blocked(self):
        adapter = adapters.Adapter('kitty', self.root, 'owned', {'fg': '#ffffff', 'bg': '#000000'}, {'font': 12})
        with self.assertRaises(Blocked):
            adapter.configure()

    def test_null_or_nonobject_font_profile_is_blocked(self):
        for profile in ({'font': None}, {'font_size': 'big'}, []):
            with self.subTest(profile=profile), self.assertRaises(Blocked):
                adapters.Adapter('kitty', self.root, 'owned', PALETTE, profile).configure()

    def main_arguments(self, name):
        binary = self.root / 'candidate'
        binary.write_bytes(b'\x7fELFprobe')
        binary.chmod(0o700)
        state = {'id': 'tui-list-text', 'steps': [], 'variants': [{'id': 'primary', 'columns': 130, 'rows': 31}]}
        spec = dict(states=[state], primary_ids=[state['id']], additional_states=[], excluded=[], reserved=[])
        arguments = ['--boards', str(self.root), '--binary', str(binary), '--terminals', 'foot', '--output', str(self.root / name)]
        return arguments, spec

    def run_with_profiles(self, document, name='out'):
        arguments, spec = self.main_arguments(name)
        profile = self.root / (name + '-profiles.json')
        profile.write_text(json.dumps(document))
        with ExitStack() as stack:
            stack.enter_context(patch.object(driver, 'contract', return_value=spec))
            stack.enter_context(patch.object(driver, 'palette', return_value=PALETTE))
            stack.enter_context(patch.object(driver, 'source_identity', return_value={'elf_sha256': 'elf'}))
            stack.enter_context(patch.object(driver.os, 'geteuid', return_value=1000))
            stack.enter_context(patch.object(adapters.Adapter, 'preflight', return_value=[]))
            stack.enter_context(patch.object(driver, 'Display', return_value=unittest.mock.MagicMock()))
            stack.enter_context(redirect_stdout(io.StringIO()))
            stack.enter_context(redirect_stderr(io.StringIO()))
            return driver.main(arguments + ['--profiles', str(profile)])

    def test_profile_typeerror_retains_manifest(self):
        code = self.run_with_profiles({'foot': {'font': 12}})
        self.assertEqual(code, 2)
        manifest = json.loads((self.root / 'out/manifest.json').read_text())
        self.assertIn('invalid private font profile', manifest['reason'])

    def test_null_or_nonobject_profile_entry_retains_manifest(self):
        for index, value in enumerate((None, [], 'x')):
            with self.subTest(value=value):
                name = f'out-{index}'
                self.assertEqual(self.run_with_profiles({'foot': value}, name), 2)
                manifest = json.loads((self.root / name / 'manifest.json').read_text())
                self.assertEqual(manifest['reason'], 'invalid private terminal profile: foot')

    def test_erased_cells_have_no_underline(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'\x1b[4m\x1b[K')
        self.assertFalse(screen.grid[0][0]['underline'])

    def test_erase_display_clears_underline(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'\x1b[4m\x1b[J')
        self.assertFalse(screen.grid[0][0]['underline'])

    def test_printed_space_keeps_underline(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'\x1b[4m ')
        self.assertEqual((screen.grid[0][0]['ch'], screen.grid[0][0]['underline']), (' ', True))

    def test_erase_keeps_background_and_clears_reverse(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'\x1b[41m\x1b[7m\x1b[K')
        self.assertEqual(screen.grid[0][0]['bg'], cells.PALETTE[1])
        self.assertFalse(screen.grid[0][0]['reverse'])

    def test_c0_in_csi_does_not_hold_later_output(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'\x1b[1\n2H')
        screen.feed(b'visible')
        self.assertFalse(screen.pending)

    def test_csi_split_at_every_byte_is_consumed_once(self):
        sequence = b'\x1b[2;3HX'
        for cut in range(1, len(sequence)):
            with self.subTest(cut=cut):
                screen = cells.Screen(6, 2)
                screen.feed(sequence[:cut])
                screen.feed(sequence[cut:])
                self.assertFalse(screen.pending)
                self.assertEqual(screen.grid[1][2]['ch'], 'X')

    def test_unparseable_completed_csi_fails_loud(self):
        with self.assertRaisesRegex(ValueError, 'unparseable terminal CSI'):
            cells.Screen(6, 2).feed(b'\x1b[\x7f')

    def test_charset_designation_does_not_print_B(self):
        screen = cells.Screen(12, 2)
        screen.feed(b'\x1b(B\x1b[mhello')
        self.assertEqual(cells.text(screen.snapshot()).splitlines()[0].strip(), 'hello')

    def test_charset_designation_split_at_every_byte(self):
        sequence = b'\x1b(Bhello'
        for cut in range(1, len(sequence)):
            with self.subTest(cut=cut):
                screen = cells.Screen(12, 2)
                screen.feed(sequence[:cut])
                screen.feed(sequence[cut:])
                self.assertEqual(cells.text(screen.snapshot()).splitlines()[0].strip(), 'hello')
                self.assertEqual(screen.unsupported, set())

    def test_wide_character_prewraps(self):
        screen = cells.Screen(6, 2)
        screen.feed(('a' * 5 + '日').encode())
        self.assertEqual(screen.grid[1][0]['ch'], '日')
        self.assertEqual(screen.grid[1][1]['width'], 0)
        self.assertEqual(screen.grid[0][5]['ch'], ' ')

    def test_wide_combining_does_not_crash(self):
        screen = cells.Screen(6, 2)
        screen.feed(('a' * 5 + '日́').encode())
        self.assertEqual(screen.grid[1][0]['ch'], '日́')

    def test_colon_rgb_preserves_channels(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'\x1b[38:2::255:0:0mR')
        self.assertEqual(screen.grid[0][0]['fg'], '#ff0000')

    def test_colon_rgb_background_preserves_channels(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'\x1b[48:2::0:255:0mR')
        self.assertEqual(screen.grid[0][0]['bg'], '#00ff00')

    def test_colon_rgb_colorspace_slot_is_consumed(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'\x1b[38:2:0:255:0:0mR')
        self.assertEqual(screen.grid[0][0]['fg'], '#ff0000')

    def test_colon_rgb_split_across_feeds(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'\x1b[38:2::255:')
        screen.feed(b'0:0mR')
        self.assertEqual(screen.grid[0][0]['fg'], '#ff0000')

    def test_erase_after_last_cell_is_bounded(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'aaaaaa\x1b[1K')
        self.assertEqual(cells.text(screen.snapshot()).splitlines()[0], ' ' * 6)

    def test_erase_to_end_at_pending_wrap_clears_last_cell(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'aaaaaa\x1b[K')
        self.assertEqual(cells.text(screen.snapshot()).splitlines()[0], 'aaaaa ')

    def test_erase_to_end_keeps_earlier_cells(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'abcdef\x1b[3G\x1b[K')
        self.assertEqual(cells.text(screen.snapshot()).splitlines()[0], 'ab    ')

    def test_erase_line_clears_whole_row(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'abcdef\x1b[2K')
        self.assertEqual(cells.text(screen.snapshot()).splitlines()[0], ' ' * 6)

    def run_candidate(self, read):
        with ExitStack() as stack:
            for name, replacement in (('os.isatty', lambda *a: True), ('termios.tcgetattr', lambda *a: []),
                                      ('tty.setraw', lambda *a: None), ('os.write', lambda *a: 1),
                                      ('termios.tcsetattr', lambda *a: None), ('os.read', read)):
                stack.enter_context(patch('tui_battery.fake.' + name, replacement))
            return fake.candidate()

    def test_candidate_eof_ends_reply_wait(self):
        reads = 0
        def read(*args):
            nonlocal reads
            reads += 1
            if reads > 4:
                raise RuntimeError('EOF read loop exceeded four iterations')
            return b''
        self.assertEqual(self.run_candidate(read), fake.FAKE_EOF_EXIT)
        self.assertLessEqual(reads, 2)

    def test_candidate_eof_after_query_reply_ends_key_wait(self):
        chunks = [bytes([byte]) for byte in b'\x1b_Gi=31;OK\x1b\\'] + [b'']
        self.assertEqual(self.run_candidate(lambda *a: chunks.pop(0)), fake.FAKE_EOF_EXIT)
        self.assertEqual(chunks, [])

    def test_exec_failure_stays_in_child_boundary(self):
        class ChildExit(BaseException):
            pass
        with ExitStack() as stack:
            stack.enter_context(patch.object(fake.pty, 'fork', return_value=(0, 0)))
            stack.enter_context(patch.object(fake.fcntl, 'ioctl', return_value=0))
            stack.enter_context(patch.object(adapters.Adapter, 'preflight', return_value=[]))
            stack.enter_context(patch.object(fake.os, 'execvpe', side_effect=FileNotFoundError('missing terminal')))
            stack.enter_context(patch.object(fake.os, '_exit', side_effect=ChildExit(127)))
            with self.assertRaises(ChildExit):
                fake.adapter_diagnostic('foot', self.root, Commands(), PALETTE)

    def fact_candidate(self, quit_action):
        script = self.root / 'fact_backend.py'
        script.write_text('import sys,json\n'
                          'for line in sys.stdin:\n'
                          ' value=json.loads(line)\n'
                          ' if value["c"]=="list": print(json.dumps({"t":"listed"}),flush=True)\n'
                          ' elif value["c"]=="fsinfo":\n'
                          '  print(json.dumps({"t":"fsinfo","path":"/fx/v/work","fs":"ext4","free":5000}),flush=True)\n'
                          '  sys.stderr.write("x"*262144);sys.stderr.flush()\n'
                          f' elif value["c"]=="quit": {quit_action}\n')
        return [sys.executable, str(script), '/opt/flea', '--tui', '/fx/v/work']

    def test_fsinfo_drains_after_result(self):
        result = dynamic.fsinfo(self.fact_candidate('break'), '/fx/v/work', Commands())
        self.assertEqual(result['free'], 5000)

    def test_fsinfo_nonzero_backend_exit_is_blocked(self):
        with self.assertRaisesRegex(Blocked, 'exited nonzero'):
            dynamic.fsinfo(self.fact_candidate('sys.exit(3)'), '/fx/v/work', Commands())

    def test_partial_exit_receipt_is_retried(self):
        adapter = unittest.mock.Mock()
        terminal = native.Terminal(None, adapter, self.root, Commands(), 1)
        (self.root / 'launch-exit.json').write_text('{')
        reads = 0
        original = Path.read_text
        def read(path, *args, **kwargs):
            nonlocal reads
            if path.name == 'launch-exit.json':
                reads += 1
                return '{' if reads == 1 else '{"exit_code":0}'
            return original(path, *args, **kwargs)
        with patch.object(Path, 'read_text', read):
            self.assertEqual(terminal.close(), {'exit_code': 0})
        adapter.close.assert_called_once()

    def test_malformed_exit_receipt_times_out_with_cleanup(self):
        adapter = unittest.mock.Mock()
        terminal = native.Terminal(None, adapter, self.root, Commands(), 1)
        (self.root / 'launch-exit.json').write_text('{')
        with self.assertRaisesRegex(Blocked, 'terminal exit and child reap'):
            terminal.close()
        adapter.close.assert_called_once()

    def media_case(self, cwd='/fx/v/work/Media'):
        fx = self.root / 'fx'
        fx.mkdir()
        directory = fx / 'v/work/Media'
        directory.mkdir(parents=True)
        (directory / 'a.png').write_bytes(b'fixture identity')
        received = media_transcript(self.root, [{'ns': 30, 'data': '\x1b_Ga=T,i=7;AAAA\x1b\\'}])
        caps = dict(forced_fallback=False, candidate_received=received)
        class Board:
            def media_boxes(self):
                return [[4, 9, 8.4, 22]]
        image = (4, 4, bytearray([0, 0, 0, 255, 100, 0, 0, 255, 0, 100, 0, 255, 0, 0, 100, 255]) * 4)
        state = {'id': 'tui-list-image', 'cwd': cwd, 'cursor': 'a.png'}
        return state, Board(), image, caps, fx

    def inspect_media(self, state, board, image, caps, fx, review=None):
        return media.inspect(state, board, {'columns': 1, 'rows': 1}, image, [0, 0, 4, 4], caps,
                             self.root, fx, self.root, review or {}, adapter='kitty', expected_transport='kitty')[0]

    def test_palette_only_crop_is_not_native_picture_proof(self):
        state, board, image, caps, fx = self.media_case()
        self.assertEqual(self.inspect_media(state, board, image, caps, fx)['status'], 'BLOCKED')

    def test_approved_native_picture_passes_for_its_identities(self):
        state, board, image, caps, fx = self.media_case()
        requirements = self.inspect_media(state, board, image, caps, fx)['review_requirements']
        review = {'representation': dict(approved=True, **requirements)}
        self.assertEqual(self.inspect_media(state, board, image, caps, fx, review)['status'], 'PASS')

    def test_native_picture_approval_is_identity_bound(self):
        state, board, image, caps, fx = self.media_case()
        requirements = self.inspect_media(state, board, image, caps, fx)['review_requirements']
        for key, value in (('fixture_sha256', 'stale'), ('native_capture_sha256', ['stale']), ('placements', [])):
            with self.subTest(key=key):
                stale = dict(requirements, **{key: value})
                result = self.inspect_media(state, board, image, caps, fx, {'representation': dict(approved=True, **stale)})
                self.assertEqual(result['status'], 'BLOCKED')

    def test_rejected_native_picture_fails(self):
        state, board, image, caps, fx = self.media_case()
        requirements = self.inspect_media(state, board, image, caps, fx)['review_requirements']
        review = {'representation': dict(approved=False, **requirements)}
        self.assertEqual(self.inspect_media(state, board, image, caps, fx, review)['status'], 'FAIL')

    def test_root_fixture_is_identity_bound(self):
        state, board, image, caps, fx = self.media_case('/fx')
        (fx / 'a.png').write_bytes(b'fixture root identity')
        result = self.inspect_media(state, board, image, caps, fx)
        self.assertEqual(result['fixture_sha256'], digest(fx / 'a.png'))

    def test_missing_current_fixture_blocks(self):
        state, board, image, caps, fx = self.media_case()
        state['cursor'] = 'absent.png'
        with self.assertRaises(Blocked):
            self.inspect_media(state, board, image, caps, fx)

    def test_fixture_outside_fx_root_is_blocked(self):
        state, board, image, caps, fx = self.media_case()
        state['cwd'] = '/home/gm'
        with self.assertRaisesRegex(Blocked, 'outside the /fx'):
            self.inspect_media(state, board, image, caps, fx)
        state.update(cwd='/fx/v/work/Media', cursor='../../../../../etc/passwd')
        with self.assertRaisesRegex(Blocked, 'escapes the /fx'):
            self.inspect_media(state, board, image, caps, fx)

    def supplement_record(self, control, assertions, name='control'):
        bundle = self.root / name
        directory = bundle / 'kitty' / control['id']
        directory.mkdir(parents=True)
        for file in receipts.REQUIRED:
            (directory / file).write_text('{}')
        (directory / 'native.png').write_bytes(b'placeholder')
        (directory / 'commands.json').write_text('[{"exit_code":0}]')
        (directory / 'pty.log').write_text('real transcript placeholder')
        (directory / 'cells.json').write_text('{"grid":[[{}]]}')
        hashes = {file: digest(directory / file) for file in receipts.REQUIRED}
        verdict = dict(behavior='PASS', cell_geometry='PASS', color='PASS', native_visual='PASS',
                       assertions=assertions, native_capture_sha256=hashes['native.png'])
        (directory / 'verdict.json').write_text(json.dumps(verdict))
        identity = dict(commit='c', dirty_patch_sha256='p', elf_sha256='e', references={}, untracked={})
        row = dict(terminal='kitty', kind='control', id=control['id'], directory='kitty/'+control['id'], sha256=hashes,
                   verdict_sha256=digest(directory / 'verdict.json'))
        (bundle / 'manifest.json').write_text(json.dumps(dict(identity=identity, records=[row])))
        # These four tests isolate assertion-shape checks; the accepted producer validator has separate integration controls.
        with patch.object(receipts.png, 'read', return_value=(1, 1, b'rgba')), \
                patch('tui_battery.control_native.validate_record', return_value={}):
            return receipts.supplement(bundle, identity, ['kitty'], [control], {'states': []})[0]

    def test_control_without_checks_cannot_pass(self):
        record = self.supplement_record({'id': 'unasserted'}, {'anything': True})
        self.assertEqual(record['status'], 'BLOCKED')

    def test_control_with_empty_checks_cannot_pass(self):
        record = self.supplement_record({'id': 'unasserted', 'checks': []}, {'anything': True}, 'empty')
        self.assertEqual(record['status'], 'BLOCKED')

    def test_missing_required_assertion_blocks(self):
        record = self.supplement_record({'id': 'unasserted', 'checks': ['required']}, {'anything': True}, 'missing')
        self.assertEqual(record['status'], 'BLOCKED')
        self.assertIn('omits required', record['reason'])

    def test_control_with_explicit_checks_passes(self):
        record = self.supplement_record({'id': 'unasserted', 'checks': ['required']}, {'required': True}, 'explicit')
        self.assertEqual(record['status'], 'PASS')

    def retained_case(self):
        helper = test_review_r2.Review2Tests()
        helper.root = self.root
        return helper, helper.retained_case()

    def test_null_shutdown_stays_inside_record_boundary(self):
        helper, (source, case, identity, row, config) = self.retained_case()
        second = dict(row, variant='secondary')
        second_case = source / 'foot/tui-list-text/secondary'
        second_case.mkdir()
        (second_case / 'media.json').write_text(json.dumps({'status': 'PASS'}))
        from .candidate import LIFECYCLE, seal_extra
        for name in LIFECYCLE:
            value = ({'exit_code': 0} if name == 'launch-exit.json' else
                     {'candidate_exit': 0, 'termios_restored': True, 'relay_termios_restored': True} if name == 'relay-exit.json' else {})
            (second_case / name).write_text(json.dumps(value))
        second_hashes = {}
        self.assertEqual(seal_extra(second_case, second_hashes), [])
        valid_shutdown = {'candidate_exit': 0, 'termios_restored': True, 'relay_termios_restored': True}
        (second_case / 'verdict.json').write_text(json.dumps(dict(
            second, behavior={'status': 'PASS'}, cells={'geometry': {'status': 'PASS'}, 'color': {'status': 'PASS'}},
            pixels={'status': 'REVIEW'}, edges={'status': 'PASS'}, terminal_exit={'exit_code': 0}, shutdown=valid_shutdown, artifact_sha256=second_hashes)))
        path = case / 'verdict.json'
        verdict = json.loads(path.read_text())
        for shutdown in (None, [], 'clean'):
            with self.subTest(shutdown=shutdown):
                verdict['shutdown'] = shutdown
                path.write_text(json.dumps(verdict))
                with patch.object(reconcile, 'verify'), \
                        patch.object(reconcile, 'visual_review', return_value={'status': 'PASS', 'review': {}}):
                    results = reconcile.retained(source, identity, [row, second], self.root, {})
                self.assertEqual(results[0]['status'], 'FAIL')
                self.assertIn('not an object', results[0]['reason'])
                self.assertEqual(results[1]['status'], 'PASS')

    def test_empty_reason_preserves_shutdown_reason(self):
        helper, (source, case, identity, row, config) = self.retained_case()
        path = case / 'verdict.json'
        verdict = json.loads(path.read_text())
        verdict.update(reason=None, shutdown_reason='tty not restored')
        path.write_text(json.dumps(verdict))
        result = helper.reconcile(source, identity, row, config)
        self.assertEqual(result['reason'], 'tty not restored')

    def test_empty_string_reason_preserves_shutdown_reason(self):
        helper, (source, case, identity, row, config) = self.retained_case()
        path = case / 'verdict.json'
        verdict = json.loads(path.read_text())
        verdict.update(reason='', shutdown_reason='tty not restored')
        path.write_text(json.dumps(verdict))
        result = helper.reconcile(source, identity, row, config)
        self.assertEqual(result['reason'], 'tty not restored')

    def test_erase_display_to_end_at_pending_wrap_clears_last_cell(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'aaaaaa\x1b[J')
        self.assertEqual(cells.text(screen.snapshot()).splitlines(), ['aaaaa ', '      '])

    def test_erase_display_to_start_at_pending_wrap_clears_through_cursor(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'aaaaaa\r\nbbbbbb\x1b[1J')
        self.assertEqual(cells.text(screen.snapshot()).splitlines(), ['      ', '      '])

    def test_erase_display_to_start_keeps_cells_after_cursor(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'aaaaaa\r\nbbbbbb\x1b[3G\x1b[1J')
        self.assertEqual(cells.text(screen.snapshot()).splitlines(), ['      ', '   bbb'])

    def test_erase_display_all_at_pending_wrap_clears_grid(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'aaaaaa\r\nbbbbbb\x1b[2J')
        self.assertEqual(cells.text(screen.snapshot()).splitlines(), ['      ', '      '])

    def test_erase_display_to_end_keeps_earlier_cells(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'abcdef\r\nghijkl\x1b[4G\x1b[J')
        self.assertEqual(cells.text(screen.snapshot()).splitlines(), ['abcdef', 'ghi   '])

    def test_erase_display_saved_lines_keeps_visible_cells(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'aaaaaa\x1b[3J')
        self.assertEqual(cells.text(screen.snapshot()).splitlines()[0], 'aaaaaa')

    def test_insert_delete_and_erase_sequences_stay_unsupported_at_pending_wrap(self):
        sequences = ((b'\x1b[X', 'CSI X'), (b'\x1b[2X', 'CSI 2X'), (b'\x1b[@', 'CSI @'), (b'\x1b[2@', 'CSI 2@'),
                     (b'\x1b[P', 'CSI P'), (b'\x1b[2P', 'CSI 2P'), (b'\x1b[L', 'CSI L'), (b'\x1b[2L', 'CSI 2L'),
                     (b'\x1b[M', 'CSI M'), (b'\x1b[2M', 'CSI 2M'))
        positions = ((b'aaaaaa', 'aaaaaa'), (b'aaa', 'aaa   '))
        for sequence, recorded in sequences:
            for text, row in positions:
                with self.subTest(sequence=sequence, text=text):
                    screen = cells.Screen(6, 2)
                    screen.feed(text + sequence)
                    snapshot = screen.snapshot()
                    self.assertIn(recorded, snapshot['unsupported'])
                    self.assertEqual(cells.text(snapshot).splitlines()[0], row)

    def widths(self, screen, row):
        return [cell['width'] for cell in screen.grid[row]]

    def wide_pair_violations(self, snapshot):
        violations = []
        for y, row in enumerate(snapshot['grid']):
            for x, cell in enumerate(row):
                if cell['width'] == 2 and (x + 1 >= len(row) or row[x + 1]['width'] != 0):
                    violations.append((y, x, 'lead without continuation'))
                if cell['width'] == 0 and (x == 0 or row[x - 1]['width'] != 2):
                    violations.append((y, x, 'continuation without lead'))
        return violations

    def test_erase_line_from_continuation_clears_whole_glyph(self):
        screen = cells.Screen(6, 2)
        screen.feed('ab中\x1b[4G\x1b[K'.encode())
        self.assertEqual(self.widths(screen, 0), [1] * 6)
        self.assertEqual(cells.text(screen.snapshot()).splitlines()[0], 'ab    ')

    def test_erase_line_to_start_through_lead_clears_continuation(self):
        screen = cells.Screen(6, 2)
        screen.feed('ab中\x1b[3G\x1b[1K'.encode())
        self.assertEqual(self.widths(screen, 0), [1] * 6)
        self.assertEqual(cells.text(screen.snapshot()).splitlines()[0], '      ')

    def test_erase_line_to_start_at_pending_wrap_clears_whole_row(self):
        screen = cells.Screen(6, 2)
        screen.feed('abcd中\x1b[1K'.encode())
        self.assertEqual(self.widths(screen, 0), [1] * 6)
        self.assertEqual(cells.text(screen.snapshot()).splitlines()[0], '      ')

    def test_erase_line_at_pending_wrap_clears_glyph_in_last_two_columns(self):
        screen = cells.Screen(6, 2)
        screen.feed('abcd中\x1b[K'.encode())
        self.assertEqual(self.widths(screen, 0), [1] * 6)
        self.assertEqual(cells.text(screen.snapshot()).splitlines()[0], 'abcd  ')

    def test_erase_display_to_end_at_pending_wrap_clears_glyph_in_last_two_columns(self):
        screen = cells.Screen(6, 2)
        screen.feed('abcd中\x1b[J'.encode())
        self.assertEqual([self.widths(screen, y) for y in range(2)], [[1] * 6, [1] * 6])
        self.assertEqual(cells.text(screen.snapshot()).splitlines()[0], 'abcd  ')

    def test_erase_display_to_start_through_lead_clears_continuation(self):
        screen = cells.Screen(6, 2)
        screen.feed('ab中\x1b[3G\x1b[1J'.encode())
        self.assertEqual(self.widths(screen, 0), [1] * 6)
        self.assertEqual(cells.text(screen.snapshot()).splitlines()[0], '      ')

    def test_erase_display_to_end_from_continuation_clears_glyph_and_rows_below(self):
        screen = cells.Screen(6, 2)
        screen.feed('ab中\r\ncdefgh\x1b[1;4H\x1b[J'.encode())
        self.assertEqual([self.widths(screen, y) for y in range(2)], [[1] * 6, [1] * 6])
        self.assertEqual(cells.text(screen.snapshot()).splitlines(), ['ab    ', '      '])

    def cells_of(self, screen, row):
        return [(cell['ch'], cell['width']) for cell in screen.grid[row]]

    def cell_violations(self, snapshot, expected):
        return [(y, x, snapshot['grid'][y][x]['ch'], snapshot['grid'][y][x]['width'], want)
                for (y, x), want in expected.items()
                if (snapshot['grid'][y][x]['ch'], snapshot['grid'][y][x]['width']) != want]

    def erase_sweep(self):
        rows = '\x1b[2;1Hcd中\x1b[3;1Hef中'
        scenarios = (
            ('lead', 2, rows + '\x1b[1;1Hab中\x1b[1;3H'),
            ('continuation', 3, rows + '\x1b[1;1Hab中\x1b[1;4H'),
            ('pending wrap', 5, rows + '\x1b[1;1Habcd中'),
        )
        sequences = (
            ('\x1b[K', lambda c: [(0, x) for x in range(c, 6)]),
            ('\x1b[1K', lambda c: [(0, x) for x in range(c + 1)]),
            ('\x1b[2K', lambda c: [(0, x) for x in range(6)]),
            ('\x1b[J', lambda c: [(0, x) for x in range(c, 6)] + [(y, x) for y in (1, 2) for x in range(6)]),
            ('\x1b[1J', lambda c: [(0, x) for x in range(c + 1)]),
            ('\x1b[2J', lambda c: [(y, x) for y in range(3) for x in range(6)]),
        )
        failures = []
        for position, column, prefix in scenarios:
            for sequence, region in sequences:
                screen = cells.Screen(6, 3)
                screen.feed((prefix + sequence).encode())
                snapshot = screen.snapshot()
                problems = self.cell_violations(snapshot, {cell: (' ', 1) for cell in region(column)})
                problems += self.wide_pair_violations(snapshot)
                if problems:
                    failures.append((position, sequence, problems))
        return failures

    def print_sweep(self):
        rows = '\x1b[2;1Hcd中\x1b[3;1Hef中'
        scenarios = (
            ('lead', 1, rows + '\x1b[1;1Ha中\x1b[1;2H'),
            ('continuation', 2, rows + '\x1b[1;1Ha中\x1b[1;3H'),
            ('before lead', 0, rows + '\x1b[1;1Ha中\x1b[1;1H'),
        )
        failures = []
        for position, column, prefix in scenarios:
            for ch, w in (('b', 1), ('九', 2)):
                screen = cells.Screen(6, 3)
                screen.feed((prefix + ch).encode())
                snapshot = screen.snapshot()
                expected = {(0, column): (ch, w)}
                if w == 2:
                    expected[(0, column + 1)] = ('', 0)
                problems = self.cell_violations(snapshot, expected) + self.wide_pair_violations(snapshot)
                if problems:
                    failures.append((position, ch, problems))
        return failures

    def test_every_erase_keeps_wide_pairs_whole(self):
        self.assertEqual(self.erase_sweep(), [])

    def test_erase_sweep_rejects_a_no_op_erase(self):
        with patch.object(cells.Screen, 'erase', lambda screen, y, start, end: None):
            self.assertTrue(self.erase_sweep())

    def test_every_print_keeps_wide_pairs_whole(self):
        self.assertEqual(self.print_sweep(), [])

    def test_print_sweep_rejects_a_no_op_print(self):
        with patch.object(cells, 'width', lambda ch: 0):
            self.assertTrue(self.print_sweep())

    def test_narrow_print_over_wide_lead_blanks_continuation(self):
        screen = cells.Screen(6, 2)
        screen.feed('中\x1b[1;1Ha'.encode())
        self.assertEqual(self.cells_of(screen, 0), [('a', 1), (' ', 1), (' ', 1), (' ', 1), (' ', 1), (' ', 1)])

    def test_narrow_print_over_wide_continuation_blanks_lead(self):
        screen = cells.Screen(6, 2)
        screen.feed('a中\x1b[1;3Hb'.encode())
        self.assertEqual(self.cells_of(screen, 0), [('a', 1), (' ', 1), ('b', 1), (' ', 1), (' ', 1), (' ', 1)])

    def test_wide_print_over_wide_continuation_blanks_previous_lead(self):
        screen = cells.Screen(6, 2)
        screen.feed('a中\x1b[1;3H九'.encode())
        self.assertEqual(self.cells_of(screen, 0), [('a', 1), (' ', 1), ('九', 2), ('', 0), (' ', 1), (' ', 1)])

    def test_wide_print_over_wide_lead_blanks_orphaned_continuation(self):
        screen = cells.Screen(6, 2)
        screen.feed('a中\x1b[1;1H九'.encode())
        self.assertEqual(self.cells_of(screen, 0), [('九', 2), ('', 0), (' ', 1), (' ', 1), (' ', 1), (' ', 1)])

    def test_wide_print_over_wide_lead_replaces_glyph(self):
        screen = cells.Screen(6, 2)
        screen.feed('a中\x1b[1;2H九'.encode())
        self.assertEqual(self.cells_of(screen, 0), [('a', 1), ('九', 2), ('', 0), (' ', 1), (' ', 1), (' ', 1)])

    def test_wide_glyph_at_last_column_wraps_and_keeps_last_cell(self):
        screen = cells.Screen(6, 2)
        screen.feed('abcde中'.encode())
        self.assertEqual(self.cells_of(screen, 0), [('a', 1), ('b', 1), ('c', 1), ('d', 1), ('e', 1), (' ', 1)])
        self.assertEqual(self.cells_of(screen, 1)[:2], [('中', 2), ('', 0)])
        self.assertEqual(screen.snapshot()['cursor'], [2, 1])
        screen = cells.Screen(6, 2)
        screen.feed('abcdef中'.encode())
        self.assertEqual(self.cells_of(screen, 0)[5], ('f', 1))
        self.assertEqual(self.cells_of(screen, 1)[:2], [('中', 2), ('', 0)])
        self.assertEqual(screen.snapshot()['cursor'], [2, 1])

    def test_private_marker_sgr_is_recorded_and_parsing_continues(self):
        for data in (b'\x1b[>4;2m', b'\x1b[>4;1m', b'\x1b[?4m'):
            with self.subTest(data=data):
                screen = cells.Screen(6, 2)
                screen.feed(data + b'ab')
                self.assertIn('CSI ' + data[2:-1].decode() + 'm', screen.unsupported)
                self.assertEqual(self.cells_of(screen, 0)[:2], [('a', 1), ('b', 1)])

    def test_colon_in_cursor_csi_is_recorded_not_parsed(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'\x1b[1:2Hx')
        self.assertIn('CSI 1:2H', screen.unsupported)
        self.assertEqual(self.cells_of(screen, 0)[0], ('x', 1))

    def test_marker_in_sgr_group_is_recorded_and_parsing_continues(self):
        screen = cells.Screen(6, 2)
        screen.feed(b'\x1b[38;?5mab')
        self.assertIn('SGR ?5', screen.unsupported)
        self.assertEqual(self.cells_of(screen, 0)[:2], [('a', 1), ('b', 1)])

    def test_unapproved_native_picture_claims_no_verification(self):
        state, board, image, caps, fx = self.media_case()
        result = self.inspect_media(state, board, image, caps, fx)
        self.assertEqual(result['status'], 'BLOCKED')
        self.assertNotIn('representation_verified', result)
        self.assertTrue(result['representation_bound'])

    def test_full_cell_background_threshold_follows_cell_width(self):
        board = references.Board.__new__(references.Board)
        board.palette = {'fg': '#c6b395', 'bg': '#1c120d', 'accent': '#ffffff'}
        board.runs = []
        shape ={'left': f'{ORIGIN[0]}px', 'top': f'{ORIGIN[1]}px', 'height': f'{CELL_HEIGHT}px',
                 'background': 'var(--accent)'}
        board.shapes = [dict(shape, width='9.7px')]
        with patch.object(references, 'CELL_WIDTH', 10.0):
            self.assertEqual(board.cells(columns=2, rows=1)[(0, 0)]['bg'], '#1c120d')
        board.shapes = [dict(shape, width=f'{references.CELL_WIDTH}px')]
        self.assertEqual(board.cells(columns=2, rows=1)[(0, 0)]['bg'], '#ffffff')

    def test_root_is_removed_after_each_test(self):
        class Probe(RegressionTests):
            def test_probe(self):
                self.assertTrue(self.root.is_dir())
        case = Probe('test_probe')
        self.assertTrue(case.run().wasSuccessful())
        self.assertFalse(case.root.exists())

    def test_nonzero_fake_exit_reports_code_and_output_tail(self):
        message = str(fake.exit_blocked('foot', fake.EXEC_FAILURE_EXIT, b'x' * 400 + b'tail'))
        self.assertTrue(message.startswith('fake foot exited 127: '))
        self.assertEqual(len(message.split(': ', 1)[1]), fake.OUTPUT_TAIL_CHARS)
        self.assertTrue(message.endswith('tail'))


if __name__ == '__main__':
    unittest.main()
