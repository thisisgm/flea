"""Guard a GPUI own-strip return when the native offer fires below the strip only once."""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

source = (Path(__file__).resolve().parent / 'ui.sh').read_text()
root = Path(tempfile.mkdtemp(prefix='flea-gpui-return.', dir='/tmp'))
(root / '.flea-test-sandbox').touch()


def function(name, optional=False):
    match = re.search(r'^' + name + r'\(\) \{\n.*?^\}', source, re.M | re.S)
    if optional and match is None:
        return ''
    assert match is not None, name
    return match[0]


try:
    helpers = '\n'.join(function(name) for name in ('xwtab_trace_lines', 'xwtab_wait_own_enter'))
    helpers += '\n' + function('xwtab_gpui_own_hit', optional=True)
    setup = r'''
set -u -o pipefail
xwtab_source=101
xwtab_marks=(1 0)
xwtab_logs=("$CONTROL_ROOT/source.log" "$CONTROL_ROOT/target.log")
xwtab_own_attempts=2
xwtab_own_poll=0.1
xwtab_own_trace_tail=10
test_frontend=gpui
xwtab_button_down=${CONTROL_HELD:-true}
fail() { printf 'FAIL %s\n' "$*"; exit 1; }
sleep() { :; }
flea_process_owned() { [[ $1 == 101 && ${CONTROL_OWNED:-true} == true ]]; }
xwdrag_assert_focus() { [[ $1 == 101 && ${CONTROL_FOCUS:-true} == true ]]; }
xwdrag_qsid() { printf '101\n'; }
xwdrag_qs() {
    case $2 in
        tabBarVisible) printf '%s\n' "${CONTROL_VISIBLE:-true}" ;;
        tabCount) printf '%s\n' "${CONTROL_COUNT:-2}" ;;
    esac
}
xwtab_tab_point() {
    [[ ${CONTROL_CENTRES:-true} == true ]] || return 1
    if [[ $3 == 0 ]]; then printf '100 120\n'; else printf '300 %s\n' "${CONTROL_SECOND_Y:-120}"; fi
}
hyprctl() { [[ $1 == cursorpos ]] && printf '%s\n' "${CONTROL_CURSOR:-80, 122}"; }
'''
    cases = (
        ('painted own strip without another offer', {}, True),
        ('pointer outside tabs', {'CONTROL_CURSOR': '800, 122'}, False),
        ('pointer below strip', {'CONTROL_CURSOR': '80, 140'}, False),
        ('malformed pointer', {'CONTROL_CURSOR': 'bad'}, False),
        ('released button', {'CONTROL_HELD': 'false'}, False),
        ('unowned source', {'CONTROL_OWNED': 'false'}, False),
        ('wrong focus', {'CONTROL_FOCUS': 'false'}, False),
        ('hidden strip', {'CONTROL_VISIBLE': 'false'}, False),
        ('wrong tab count', {'CONTROL_COUNT': '1'}, False),
        ('missing painted centres', {'CONTROL_CENTRES': 'false'}, False),
        ('inconsistent centre heights', {'CONTROL_SECOND_Y': '140'}, False),
        ('stale platform start', {'CONTROL_START': 'false'}, False),
        ('foreign platform start', {'CONTROL_PID': '1010'}, False),
        ('finished before return', {'CONTROL_FINISHED': 'true'}, False),
    )
    for name, updates, success in cases:
        trace = 'TABDRAG enter-strip pid=101 ok=true stale=true\n'
        if updates.get('CONTROL_START') != 'false':
            trace += 'TABDRAG drag-start pid=' + updates.get('CONTROL_PID', '101') + ' index=1\n'
        if updates.get('CONTROL_FINISHED') == 'true':
            trace += 'TABDRAG drag-finished pid=101 action=0\n'
        (root / 'source.log').write_text(trace)
        (root / 'target.log').write_text('')
        command = setup + helpers + '\nxwtab_wait_own_enter ' + str(len(trace.splitlines())) + ' 0\n'
        result = subprocess.run(['bash', '-c', command], env=dict(os.environ, CONTROL_ROOT=str(root), **updates),
                                capture_output=True, text=True, timeout=3)
        assert (result.returncode == 0) == success, (name, result.stdout, result.stderr)
        print('XWTAB GPUI RETURN ' + name + ' PASS', flush=True)
    print('XWTAB_GPUI_RETURN checks=' + str(len(cases)) + ' failed=0', flush=True)
finally:
    assert root.is_absolute() and str(root) and root.parent == Path('/tmp') and (root / '.flea-test-sandbox').is_file()
    shutil.rmtree(root)
