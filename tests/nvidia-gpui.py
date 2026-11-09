#!/usr/bin/env python3
"""The tester report must redact paths and refuse invented timing or unsafe cleanup."""
from pathlib import Path
import contextlib
import io
import json
import os
import runpy
import tempfile
import unittest
from unittest.mock import patch

report = runpy.run_path(str(Path(__file__).resolve().parents[1] / "tools/flea-gpu-report"))


class GpuReport(unittest.TestCase):
    def test_summary_has_no_personal_paths_or_raw_log_fields(self):
        adapter = dict(name='NVIDIA RTX\n/home/private/identity', driver='nvidia',
                       version='610.57 /home/private/build.log', cpu=False)
        frames = dict(first_ms=123.5, intervals_ms=[10., 20., 30., 40., 50.])
        text = report['summary'](['0x10de:0x1234 nvidia 610.57'], 'Wayland', adapter, frames, 'complete')
        self.assertNotIn('/home', text)
        self.assertNotIn('private', text)
        self.assertIn('CPU renderer: no', text)
        self.assertIn('median 30.00 ms, p95 50.00 ms', text)
        self.assertIn('First frame: 123.50 ms', text)

    def test_cpu_report_is_loud_and_unknown_is_not_no(self):
        cpu = report['summary']([], 'Wayland', dict(cpu=True), {}, 'complete')
        unknown = report['summary']([], 'X11', {}, {}, 'failed')
        self.assertIn('YES, CPU RENDERER IN USE', cpu)
        self.assertIn('CPU renderer: unknown', unknown)
        self.assertIn('unavailable (no presentation feedback)', unknown)

    def test_current_capture_and_s6_backend_line_use_actual_adapter(self):
        raw = '\n'.join([
            'flea: Vulkan is unusable, no usable driver, so the window starts on OpenGL ("Intel UHD 630")',
            'GPUI_RENDERER backend=gl adapter="Intel UHD 630" driver="" driver_info="Mesa 26.2" software=false scale=1 text_size=14 reduced_motion=true',
        ])
        adapter, frames, _ = report['decode'](raw)
        self.assertEqual(adapter['name'], 'Intel UHD 630')
        self.assertFalse(adapter['cpu'])
        self.assertEqual(adapter['driver'], '')
        self.assertEqual(adapter['backend'].lower(), 'gl')
        self.assertEqual(frames, {})
        raw = 'GPUI_RENDERER backend=vulkan adapter="llvmpipe" driver="llvmpipe" driver_info="Mesa" software=true scale=1 text_size=14 reduced_motion=true'
        adapter, _, _ = report['decode'](raw)
        self.assertTrue(adapter['cpu'])
        self.assertEqual(adapter['device_type'], 'Cpu')

    def test_nvidia_hooks_final_adapter_keeps_s6_backend_and_candidate_build(self):
        for cpu, name in [(False, 'Intel UHD 630'), (True, 'llvmpipe')]:
            raw = '\n'.join([
                f'flea: the window starts on Vulkan ("{name}")',
                'FLEA_GPU_ADAPTER ' + json.dumps(dict(name=name, driver='Mesa', version='26.2',
                                                     cpu=cpu, build='0.4.0 release')),
            ])
            adapter, _, _ = report['decode'](raw)
            self.assertEqual(adapter['backend'], 'Vulkan')
            self.assertEqual(adapter['build'], '0.4.0 release')
            self.assertEqual(adapter['cpu'], cpu)
            self.assertEqual(adapter['device_type'], 'Cpu' if cpu else 'hardware (exact type unavailable)')

    def test_nvidia_hooks_adapter_change_remains_partial_after_final_snapshot(self):
        raw = '\n'.join([
            'flea: the window starts on Vulkan ("Nvidia")',
            'FLEA_GPU_ADAPTER {"name":"Nvidia","cpu":false}',
            'flea: the window starts on Vulkan ("llvmpipe")',
            'FLEA_GPU_ADAPTER {"name":"llvmpipe","cpu":true}',
        ])
        adapter, _, _ = report['decode'](raw)
        self.assertEqual(adapter['name'], 'llvmpipe')
        self.assertTrue(adapter.get('changed'), 'final specs must not erase an adapter change during measured scrolling')
        self.assertIsNone(adapter['cpu'])

    def test_non_object_adapter_diagnostics_are_rejected_before_use(self):
        raw = 'FLEA_GPU_ADAPTER "Intel"\nflea: the window starts on Vulkan ("Intel")'
        with self.assertRaises(ValueError):
            report['decode'](raw)

    def test_non_string_backend_diagnostics_are_rejected_before_use(self):
        raw = '\n'.join([
            'flea-graphics:   - Intel (vendor=0x8086, device=0x1234, backend=Vulkan, type=IntegratedGpu)',
            'FLEA_GPU_ADAPTER {"name":"Intel","cpu":false,"backend":7}',
        ])
        with self.assertRaises(ValueError):
            report['decode'](raw)

    def test_only_structured_candidate_diagnostics_are_read(self):
        raw = 'home=/home/private\nflea-graphics:   - Intel (vendor=0x8086, device=0x1234, backend=Vulkan, type=IntegratedGpu)\nFLEA_GPU_ADAPTER {"name":"Intel","cpu":false}\nFLEA_GPU_FRAMES {"complete":true,"intervals_ms":[10,20]}\n'
        adapter, frames, policy = report['decode'](raw)
        self.assertEqual(adapter, dict(name='Intel', cpu=False, backend='Vulkan', device_type='IntegratedGpu'))
        self.assertTrue(frames['complete'])
        self.assertEqual(policy, 'unavailable')

    def test_same_gpu_name_uses_the_backend_that_actually_opened(self):
        raw = '\n'.join([
            'flea-graphics:   - Intel (vendor=0x8086, device=0x1234, backend=Vulkan, type=IntegratedGpu)',
            'flea-graphics: Selected GPU (passed configuration test): Intel (Vulkan)',
            'flea-graphics:   - Intel (vendor=0x8086, device=0x1234, backend=Gl, type=IntegratedGpu)',
            'flea-graphics: Selected GPU (passed configuration test): Intel (Gl)',
            'FLEA_GPU_ADAPTER {"name":"Intel","cpu":false}',
        ])
        adapter, _, _ = report['decode'](raw)
        self.assertEqual(adapter['backend'], 'Gl')

    def test_private_manifest_names_and_filters_are_withheld(self):
        self.assertEqual(report['icd_name']('alice-private-driver.json'), 'other ICD (filename withheld)')
        self.assertEqual(report['icd_name']('nvidia_icd.json'), 'nvidia_icd.json')
        _, _, policy = report['decode']('flea-gpu-policy: excluded="alice-private-driver.json" operator_icd=false')
        self.assertNotIn('alice', policy)
        self.assertIn('filename withheld', policy)

    def test_policy_names_the_idle_nvidia_decision(self):
        _, _, policy = report['decode']('flea-gpu-policy: excluded="nvidia_icd.json" operator_icd=false nvidia_idle_disabled=true selection=compositor-feedback')
        self.assertIn('idle NVIDIA drivers disabled=yes', policy)
        _, _, policy = report['decode']('flea-gpu-policy: excluded="" operator_icd=false nvidia_idle_disabled=false selection=compositor-feedback')
        self.assertIn('idle NVIDIA drivers disabled=no', policy)
        _, _, policy = report['decode']('flea-gpu-policy: excluded="" operator_icd=false')
        self.assertIn('idle NVIDIA drivers disabled=unknown', policy)

    def test_two_early_frames_do_not_hide_a_stalled_scroll(self):
        self.assertFalse(report['scroll_covered'](dict(intervals_ms=[16, 17])))
        self.assertFalse(report['scroll_covered'](dict(intervals_ms=[float('nan'), float('inf')])))
        self.assertFalse(report['scroll_covered'](dict(intervals_ms=[True] * 5000)))
        self.assertTrue(report['scroll_covered'](dict(intervals_ms=[16.7] * 298)))

    def test_adapter_recovery_does_not_keep_claiming_the_initial_cpu_state(self):
        raw = '\n'.join([
            'flea-graphics: Selected GPU (passed configuration test): Nvidia (Vulkan)',
            'FLEA_GPU_ADAPTER {"name":"Nvidia","cpu":false}',
            'flea-graphics: Selected GPU (passed configuration test): llvmpipe (Vulkan)',
        ])
        adapter, _, _ = report['decode'](raw)
        self.assertTrue(adapter['changed'])
        self.assertIsNone(adapter['cpu'])

    def test_installed_command_isolated_launch_and_private_log_filter(self):
        with tempfile.TemporaryDirectory(prefix='flea-gpu-report-command-') as raw:
            root = Path(raw)
            assert root.is_absolute() and str(root)
            (root / '.flea-test-sandbox').write_text('report command fixture\n')
            for name in ('cache', 'config', 'data'):
                (root / name).mkdir()
            executable = root / 'flea'
            executable.write_text("""#!/usr/bin/env python3
import json, os, pathlib, sys
for key in ('XDG_CACHE_HOME', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_RUNTIME_DIR'):
    assert pathlib.Path(os.environ[key]).is_dir()
assert len(list(pathlib.Path(sys.argv[2]).iterdir())) == 10000
assert 'FLEA_UNLOCK' not in os.environ
assert 'SOL_NVIDIA_SECRET_CONTROL' not in os.environ
assert pathlib.Path(os.environ['HOME']).is_dir()
assert os.stat('/dev/shm').st_ino == os.stat(pathlib.Path(os.environ['HOME']).parent / 'shm').st_ino
outside = pathlib.Path(__file__).with_name('write-outside-report-root')
assert outside.is_absolute() and str(outside)
try:
    outside.write_text('must be refused')
except OSError:
    pass
else:
    raise AssertionError('write outside report root accepted')
print('/home/private/raw-driver-log', file=sys.stderr)
print('FLEA_GPU_ADAPTER ' + json.dumps(dict(name='Intel', driver='Mesa', version='test', cpu=False)), file=sys.stderr)
print('FLEA_GPU_FRAMES ' + json.dumps(dict(first_ms=25, complete=True, ready=True, intervals_ms=[16.7] * 298)), file=sys.stderr)
""")
            executable.chmod(0o700)
            previous = dict(os.environ)
            os.environ.update(XDG_CACHE_HOME=str(root / 'cache'), XDG_CONFIG_HOME=str(root / 'config'),
                              XDG_DATA_HOME=str(root / 'data'), FLEA_UNLOCK='/dev/do-not-touch',
                              SOL_NVIDIA_SECRET_CONTROL='must-not-reach-candidate')
            try:
                output = io.StringIO()
                with contextlib.redirect_stdout(output):
                    status = report['run'](str(executable))
                text = output.getvalue()
                self.assertEqual(status, 0, text)
                self.assertNotIn('/home', text)
                self.assertNotIn(str(root), text)
                self.assertIn('Result: complete', text)
            finally:
                os.environ.clear()
                os.environ.update(previous)

    def test_cleanup_refuses_outside_root_and_unmarked_directory(self):
        for bad in (Path('/'), Path(''), Path('/home/private')):
            with self.assertRaises(RuntimeError):
                report['cleanup'](bad)
        with tempfile.TemporaryDirectory(prefix='flea-gpu-report-') as raw:
            with self.assertRaises(RuntimeError):
                report['cleanup'](Path(raw).resolve())
            assert Path(raw).is_absolute() and raw
            (Path(raw) / '.flea-test-sandbox').write_text('cleanup fixture\n')
        print('negative control: cleanup outside marked temporary root refused')


class ReportRegression(unittest.TestCase):
    def candidate(self, flags=True, authority=False, relative=False, markers=False, changed=False):
        with tempfile.TemporaryDirectory(prefix='flea-nvidia-review-', dir='/tmp') as raw:
            root = Path(raw)
            (root / '.flea-test-sandbox').write_text('review regression fixture\n')
            home = root / 'home'
            home.mkdir()
            (home / '.Xauthority').write_text('synthetic-authority-control')
            binary = root / 'flea'
            check = """authority_file = pathlib.Path(os.environ['XAUTHORITY'])
assert authority_file.read_text() == 'synthetic-authority-control'
try:
    authority_file.write_text('must be refused')
except OSError:
    pass
else:
    raise AssertionError('Xauthority is writable')
""" if authority else ''
            if markers:
                check += "assert os.environ.get('FLEA_VK_PIN') == '1'\nassert os.environ.get('FLEA_VK_DISABLE') == 'nvidia_icd.json'\n"
                check += "assert os.environ.get('ZED_DEVICE_ID') == '27e0'\nassert os.environ.get('DRI_PRIME') == '1'\nassert os.environ.get('__NV_PRIME_RENDER_OFFLOAD') == '1'\n"
            initial = "print('flea-graphics: Selected GPU (passed configuration test): Intel (Vulkan)', file=sys.stderr)\n" if changed else ''
            recovered = "print('flea-graphics: Selected GPU (passed configuration test): llvmpipe (Vulkan)', file=sys.stderr)\n" if changed else ''
            binary.write_text('#!/usr/bin/env python3\nimport os, pathlib, sys\n' + check +
                initial +
                'print(\'FLEA_GPU_ADAPTER {"name":"Intel","cpu":false}\', file=sys.stderr)\n' +
                recovered +
                'print(' + repr('FLEA_GPU_FRAMES ' + json.dumps(dict(first_ms=25, complete=flags, ready=flags, intervals_ms=[16.7] * 298))) + ', file=sys.stderr)\n')
            binary.chmod(0o700)
            previous = dict(os.environ)
            previous_cwd = Path.cwd()
            try:
                os.environ.pop('WAYLAND_DISPLAY', None)
                os.environ.pop('XAUTHORITY', None)
                os.environ.update(HOME=str(home), DISPLAY=':4242')
                if markers:
                    os.environ.update(FLEA_VK_PIN='1', FLEA_VK_DISABLE='nvidia_icd.json',
                                      VK_DRIVER_FILES='/parent/lvp.json', VK_LOADER_DRIVERS_DISABLE='nvidia_icd.json',
                                      ZED_DEVICE_ID='27e0', DRI_PRIME='1', __NV_PRIME_RENDER_OFFLOAD='1')
                output = io.StringIO()
                with contextlib.redirect_stdout(output):
                    if relative:
                        os.chdir(root)
                    status = report['run']('./flea' if relative else str(binary))
                self.last_output = output.getvalue()
                return status
            finally:
                os.environ.clear()
                os.environ.update(previous)
                os.chdir(previous_cwd)

    def test_completion_string_false_never_means_complete(self):
        self.assertEqual(self.candidate(flags='false'), 1)

    def test_backend_actual_snapshot_backend_beats_enumeration_order(self):
        raw = '\n'.join([
            'flea-graphics:   - Intel (vendor=0x8086, device=0x1234, backend=Vulkan, type=IntegratedGpu)',
            'flea-graphics:   - Intel (vendor=0x8086, device=0x1234, backend=Gl, type=IntegratedGpu)',
            'GPUI_RENDERER backend=gl adapter="Intel" driver="" driver_info="Mesa" software=false scale=1',
        ])
        adapter, _, _ = report['decode'](raw)
        self.assertEqual(adapter['backend'].lower(), 'gl')

    def test_authentication_default_xauthority_is_preserved_readonly(self):
        self.assertEqual(self.candidate(authority=True), 0)

    def test_cleanup_cleanup_does_not_depend_on_operator_tmpdir(self):
        with tempfile.TemporaryDirectory(prefix='flea-gpu-report-', dir='/tmp') as raw:
            root = Path(raw)
            (root / '.flea-test-sandbox').write_text('cleanup control\n')
            with patch.object(tempfile, 'tempdir', '/run/user/1000'):
                report['cleanup'](root)
            self.assertFalse(root.exists())

    def test_inventory_manifest_cap_counts_before_redaction(self):
        with tempfile.TemporaryDirectory(prefix='flea-nvidia-icds-', dir='/tmp') as raw:
            root = Path(raw)
            (root / '.flea-test-sandbox').write_text('manifest cap fixture\n')
            icd = root / 'config/vulkan/icd.d'
            icd.mkdir(parents=True)
            for index in range(257):
                (icd / f'private-{index}.json').write_text('{}')
            env = dict(HOME=str(root), XDG_CONFIG_HOME=str(root / 'config'))
            self.assertEqual(report['icds'](env), ['inventory exceeds 256 manifests'])

    def test_inherited_policy_keeps_ownership_markers_and_gpu_selectors(self):
        self.assertEqual(self.candidate(markers=True), 0)

    def test_relative_executable_is_resolved_before_chdir(self):
        self.assertEqual(self.candidate(relative=True), 0)

    def test_adapter_change_reports_partial_and_unknown_cpu(self):
        self.assertEqual(self.candidate(changed=True), 1)
        self.assertIn('PARTIAL: adapter changed', self.last_output)
        self.assertIn('CPU renderer: unknown', self.last_output)


if __name__ == '__main__':
    unittest.main()
