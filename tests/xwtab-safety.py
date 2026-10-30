#!/usr/bin/env python3
# Run shipped shell helpers against owned-window, receiver, and trace doubles.
import pathlib
import re
import shlex
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
UI = (ROOT / 'tests/ui.sh').read_text()
SHELL_TIMEOUT_SECONDS = 10
LUA_PREFIX = 'hl.' + 'dsp.'
TYPED_LIBRARY = '. ' + shlex.quote(str(ROOT / 'tests/lib/hypr-dispatch.sh')) + '\n'
checks = 0
failures = 0


# Sample input: "probe documents LAYERDROP FAIL <why>" prints as "probe documents LAYERDROP failed <why>".
FAIL_WORD = re.compile(r'FAIL(?=[: ])')


def check(name, condition, detail=''):
    global checks, failures
    checks += 1
    if condition:
        # An ok line may name a FAIL marker it proves, and the CI contract reads that word as a verdict.
        print('ok ' + FAIL_WORD.sub('failed', name))
    else:
        failures += 1
        print('FAIL ' + name + ': ' + detail)


# Sample input: "xwtab_release() {\n    ydotool click 0x80\n}".
def function(text, name):
    start = ('\n' + text).find('\n' + name + '() {')
    if start < 0:
        raise ValueError('missing shell helper: ' + name)
    line = text[start:text.index('\n', start)]
    if line.endswith('}'):
        return line
    return text[start:text.index('\n}', start) + 2]


for helper in ('xwtab_wait_cancel', 'layerdrop_wait_drag'):
    try:
        function('', helper)
    except ValueError as error:
        check('missing helper refused by name: ' + helper, helper in str(error), str(error))
    else:
        check('missing helper refused by name: ' + helper, False, 'extraction succeeded')


def shell(code):
    return subprocess.run(['bash', '-c', code], cwd=ROOT, capture_output=True, text=True, timeout=SHELL_TIMEOUT_SECONDS)


STACKED_COMMENT_FILES = ('tests/xwtab-norm.sh', 'tests/xwtab-scan.sh')
RECEIVER_CONSTANT_COUNT = 2
DRAGWIRE_HEADER_LINES = 1
DRAGWIRE_HEADER_TOPIC = 'tab drag'
TEAROFF_HEADER_LINES = 2
PARSER_SAMPLE_LEAD = 2
BARE_BOUND = re.compile(r'seq 1 \d|hl\.dsp\.window\.(?:move|resize)\([^)]*[xy] = \d|"\d+ \d+ \d+ \d+ (?:True|False)"'
                        r'|hypr_window_(?:move|resize)(?:_absolute)? +\S+ +(?:-?\d+(?: |$)|\S+ +-?\d+(?: |$))')
BARE_SUBPROCESS_TIMEOUT = re.compile('timeout=' + r'\d')
BARE_TIMER_INTERVAL = re.compile(r'interval:\s*\d')


# Sample input: `    if [[ "$mode" == catcher ]]; then` ... `    else\n        xwtab_wait_enter "$bpid" "$mode"\n    fi`, one ladder.
def routes_modes(body):
    return re.search(r'\n    if \[\[ "\$mode" == catcher \]\]; then\n        xwtab_wait_catcher\n'
                     r'    elif \[\[ "\$mode" == own \]\]; then\n        xwtab_wait_own_enter [^\n]*\n'
                     r'    elif \[\[ "\$mode" == refused \]\]; then\n        xwtab_wait_refused "\$bpid" held\n'
                     r'    else\n        xwtab_wait_enter "\$bpid" "\$mode"\n    fi\n', body) is not None


# Sample input: `        xwtab_wait_enter "$bpid" "$mode"` is one call however its arguments are quoted; `xwtab_wait_enter() {` is none.
def enter_wait_calls(text):
    return len(re.findall(r'\bxwtab_wait_enter\b(?!\(\))', text))


# Sample input: "# one\n# two\ncode\n# three" has the comment runs [(1, 2)], and a lone comment is no run.
def comment_runs(text):
    runs = []
    start = length = 0
    for number, line in enumerate(text.split('\n') + [''], 1):
        if line.lstrip().startswith('#') and not line.startswith('#!'):
            start = start or number
            length += 1
            continue
        if length > 1:
            runs.append((start, length))
        start = length = 0
    return runs


# Sample input: xwtab_receiver_x=1100 and xwtab_receiver_y=500 are the receiver's placement constants in ui.sh.
def receiver_placement(text):
    return re.findall(r'^xwtab_receiver_[xy]=[0-9]+$', text, re.MULTILINE)


# Sample input: "#!/bin/bash\n# Guards a tab drag.\n# Second line.\nset -u" has the header ["# Guards a tab drag.", "# Second line."].
def header_comments(text):
    header = []
    for line in text.split('\n')[1:]:
        if not line.startswith('#'):
            break
        header.append(line)
    return header


# Sample input: "x=$(python3 -c '\nprint(json.load(f))\n')" has no sample comment, so its line 2 is returned.
def unsampled_parsers(text):
    lines = text.split('\n')
    missing = []
    for index, line in enumerate(lines):
        if 'json.load' not in line:
            continue
        start = index
        while start > 0 and 'python3 -' not in lines[start]:
            start -= 1
        window = lines[max(start - PARSER_SAMPLE_LEAD, 0):index + 1]
        if not any('Sample input' in entry for entry in window):
            missing.append(index + 1)
    return missing


# Sample input: "seq 1 40", "window.move({ x = 40," and "hypr_window_move_absolute $addr 40 80" hold a bare bound, while seq 1 "$n" and x = $px do not.
def bare_bounds(text):
    return [number for number, line in enumerate(text.split('\n'), 1)
            if not line.lstrip().startswith('#') and BARE_BOUND.search(line)]


def cache_snapshot(path):
    return path.exists(), {
        entry.relative_to(path).as_posix(): (entry.is_dir(), entry.stat().st_mtime_ns,
                                            entry.read_bytes() if entry.is_file() else None)
        for entry in path.rglob('*')
    }


with tempfile.TemporaryDirectory() as temporary:
    scratch = pathlib.Path(temporary)
    git = ['git', '-C', str(ROOT)]
    # The verified CI archive has no Git metadata, so index its files in private scratch.
    if not (ROOT / '.git').exists():
        metadata = scratch / 'git'
        subprocess.run(['git', 'init', '-q', str(metadata)], check=True, capture_output=True,
                       timeout=SHELL_TIMEOUT_SECONDS)
        git = ['git', '--git-dir=' + str(metadata / '.git'), '--work-tree=' + str(ROOT)]
        subprocess.run(git + ['add', '-A', '-f', '--', '.', ':(exclude)target', ':(exclude).superpowers'],
                       cwd=ROOT, check=True, capture_output=True, timeout=SHELL_TIMEOUT_SECONDS)
    tracked = subprocess.run(git + ['ls-files', '-z'], cwd=ROOT, capture_output=True, text=True,
                             timeout=SHELL_TIMEOUT_SECONDS)
    bytecode = [path for path in tracked.stdout.split('\0') if path.endswith('.pyc')]
    check('no tracked Python bytecode', tracked.returncode == 0 and not bytecode,
          tracked.stderr + repr(bytecode))
    scan = (ROOT / 'tests/xwtab-scan.sh').read_text()
    start = scan.rfind('\n', 0, scan.index("<<'PYSHAPES'")) + 1
    end = scan.index('\nfi', start) + len('\nfi')
    cache = ROOT / 'tests/__pycache__'
    cache_before = cache_snapshot(cache)
    result = shell('repo=' + shlex.quote(str(ROOT)) + '\nok() { :; }\nbad() { exit 1; }\n' + scan[start:end])
    check('scan import succeeds', result.returncode == 0, result.stdout + result.stderr)
    check('scan adds nothing to tests/__pycache__', cache_snapshot(cache) == cache_before,
          result.stdout + result.stderr)
    fixture_root = scratch / 'cache-fixture'
    fixture_tests = fixture_root / 'tests'
    fixture_tests.mkdir(parents=True)
    (fixture_tests / 'xwtab_free_point.py').write_text((ROOT / 'tests/xwtab_free_point.py').read_text())
    fixture_cache = fixture_tests / '__pycache__'
    for existing in (False, True):
        if existing:
            fixture_cache.mkdir()
            (fixture_cache / 'preexisting.pyc').write_bytes(b'preexisting cache')
        before = cache_snapshot(fixture_cache)
        result = shell('repo=' + shlex.quote(str(fixture_root)) + '\nok() { :; }\nbad() { exit 1; }\n' + scan[start:end])
        check('scan import preserves cache with existing=' + str(existing),
              result.returncode == 0 and cache_snapshot(fixture_cache) == before,
              result.stdout + result.stderr)
    (fixture_cache / 'added.pyc').write_bytes(b'new bytecode')
    check('cache snapshot detects bytecode added beside a preexisting entry',
          cache_snapshot(fixture_cache) != before)
    log = scratch / 'calls'
    receiver_start = UI.index('    [[ -n "$recv_addr" ]] || fail')
    receiver_end = UI.index('    read -r rcx rcy', receiver_start)
    receiver = 'receiver_case() {\n' + UI[receiver_start:receiver_end] + '\n}\n'
    focus_constants = UI[UI.index('xwdrag_focus_attempts='):UI.index('# Bounded wait until the active window')]
    focus_helpers = focus_constants + '\n'.join(function(UI, name) for name in
                                                ('xwdrag_active_field', 'xwdrag_addr', 'xwdrag_focus',
                                                 'xwdrag_assert_focus', 'xwdrag_wait_focus',
                                                 'xwdrag_fail_unfocused'))
    # The typed compositor helpers own every focus, float and move the receiver case issues.
    focus_helpers += '\n' + TYPED_LIBRARY
    receiver_constants = receiver_placement(UI)
    check('receiver placement constants are exactly the two bare assignments',
          len(receiver_constants) == RECEIVER_CONSTANT_COUNT, repr(receiver_constants))
    focus_helpers += '\n' + '\n'.join(receiver_constants)
    if 'hypr_dispatch() {' in UI:
        focus_helpers += '\n' + function(UI, 'hypr_dispatch')
    for refusal in ('none', 'focus-status', 'focus-reply', 'focus-stolen',
                    'float-status', 'float-reply', 'move-status', 'move-reply'):
        log.write_text('')
        result = shell(focus_helpers + '\n' + receiver + f"\ncall_log='{log}'\nrefusal='{refusal}'\nlua_prefix={LUA_PREFIX}\n" + r'''
recv_pid=303
recv_addr=0xc
fail() {
    printf 'FAIL %s\n' "$*"
    exit 1
}
sleep() { :; }
hyprctl() {
    case "$1" in
        clients)
            printf '%s\n' '[{"pid":303,"address":"0xc","title":"flea-drag-receiver"}]'
            ;;
        activewindow)
            printf 'readback\n' >> "$call_log"
            if [[ "$refusal" == focus-stolen ]]; then
                printf '%s\n' '{"pid":202,"address":"0xb"}'
            else
                printf '%s\n' '{"pid":303,"address":"0xc"}'
            fi
            ;;
        *)
            printf '%s\n' "$2" >> "$call_log"
            local phase
            case "$2" in
                "$lua_prefix"focus*) phase=focus ;;
                "$lua_prefix"window.float*) phase=float ;;
                "$lua_prefix"window.move*) phase=move ;;
            esac
            if [[ "$refusal" == "$phase-status" ]]; then
                return 1
            elif [[ "$refusal" == "$phase-reply" ]]; then
                printf 'refused\n'
            else
                printf 'ok\n'
            fi
            ;;
    esac
}
if (
    set -e
    receiver_case
); then
    exit 0
else
    exit 1
fi
''')
        calls = log.read_text().splitlines()
        actions = [line for line in calls if LUA_PREFIX + 'window.' in line]
        detail = result.stdout + result.stderr + ' calls=' + repr(calls)
        if refusal == 'none':
            check('receiver placement reads focus and explicitly targets its address',
                  result.returncode == 0 and 'readback' in calls and len(actions) == 2
                  and all('window = "address:0xc"' in line for line in actions), detail)
        else:
            check('receiver placement fails loudly on ' + refusal,
                  result.returncode != 0 and 'FAIL' in result.stdout
                  and len(actions) <= (1 if refusal.startswith('float') else 2 if refusal.startswith('move') else 0), detail)

    cleanup_function = function(UI, 'xwtab_cleanup')

    def cleanup_kills(code):
        log.write_text('')
        run = shell(code + f'''
recv_pid=345
xwtab_release() {{ :; }}
xwtab_restore_place() {{ :; }}
kill() {{ printf '%s\\n' "$*" >> '{log}'; }}
wait() {{ :; }}
xwtab_cleanup
''')
        return run, log.read_text().splitlines()

    result, kills = cleanup_kills(cleanup_function)
    check('failure cleanup kills exactly the saved receiver pid', result.returncode == 0 and kills == ['345'],
          result.stdout + result.stderr + repr(kills))
    wrong_pid = cleanup_function.replace('kill "$recv_pid"', 'kill "1$recv_pid"')
    result, kills = cleanup_kills(wrong_pid)
    check('cleanup check refuses a kill of the wrong pid', wrong_pid != cleanup_function and kills == ['1345'],
          result.stdout + result.stderr + repr(kills))

    room_helpers = UI[UI.index('xwtab_rect_of() {'):UI.index('# xw6: a tab dragged onto another Flea')]
    double = f'''
repo='{ROOT}'
. "$repo/tests/lib/hypr-dispatch.sh"
xwtab_saved=''
xwtab_cleanup() {{ :; }}
fail() {{ echo "FAIL $*"; exit 1; }}
flea_process_owned() {{ [[ "$1" == 101 || "$1" == 202 ]]; }}
sleep() {{ :; }}
hyprctl() {{
    if [[ "$1" == monitors ]]; then
        printf '%s\\n' '[{{"name":"DP-2","x":0,"y":0,"width":1000,"height":800,"focused":true,"activeWorkspace":{{"id":1}}}}]'
    elif [[ "$1" == clients ]]; then printf '[]\\n'
    elif [[ "$1" == layers ]]; then printf '%s\\n' '{{"DP-2":{{"levels":{{"1":[]}}}}}}'
    else
        printf '%s\\n' "$2" >> '{log}'
        printf 'ok\\n'
    fi
}}
xwtab_rect_of() {{
    local addr x y w h
    if [[ "$1" == 101 ]]; then addr=0xa; x=20; else addr=0xb; x=510; fi
    y=20; w=470; h=370
    if [[ "${{stale:-false}}" == true || ! -s '{log}' ]]; then x=0; y=0; w=900; h=500; fi
    printf '%s %s %s %s %s True\\n' "$addr" "$x" "$y" "$w" "$h"
}}
'''
    expected_room = {'0xa': ['float', 'resize', 'move'], '0xb': ['float', 'resize', 'move']}

    # Sample input: hl.dsp.window.resize({ x = 470, y = 370, relative = false, window = "address:0xa" }).
    def room_run(helpers):
        log.write_text('')
        run = shell(helpers + double + '\nxwtab_make_room 101 202\n')
        calls = log.read_text()
        per_window = {}
        for line in calls.splitlines():
            action = re.search(r'hl\.dsp\.window\.(float|resize|move)\(.*window = "address:(0x[0-9a-f]+)"', line)
            if action:
                per_window.setdefault(action.group(2), []).append(action.group(1))
        every_addressed = all('window = "address:' in line for line in calls.splitlines() if LUA_PREFIX + 'window.' in line)
        return run, calls, per_window, every_addressed

    result, calls, per_window, every_addressed = room_run(room_helpers)
    check('room operations target owned addresses despite ignored focus',
          result.returncode == 0 and per_window == expected_room and every_addressed, calls + result.stderr)
    without_geometry = '\n'.join(line for line in room_helpers.split('\n')
                                 if 'hypr_window_resize_absolute "$addr" "$pw" "$ph"' not in line
                                 and 'hypr_window_move_absolute "$addr" "$px"' not in line)
    result, calls, per_window, every_addressed = room_run(without_geometry)
    check('room check refuses a run that never resized or moved a window',
          without_geometry != room_helpers and result.returncode == 0 and per_window != expected_room,
          calls + result.stderr + repr(per_window))
    result = shell(room_helpers + double + '\nstale=true\nxwtab_make_room 101 202\n')
    check('room fails when geometry never moved', result.returncode != 0, result.stdout + result.stderr)

    log.write_text('')
    result = shell(room_helpers + double + '''
xwtab_saved='101 0xa 20 20 470 370 False
999 0xforeign 1 2 3 4 True'
xwtab_restore_place
''')
    calls = log.read_text()
    actions = [line for line in calls.splitlines() if LUA_PREFIX + 'window.' in line]
    check('restore touches only proven owned address', actions and all('window = "address:0xa"' in line for line in actions), calls + result.stderr)

    duplicate_clients = ('[{"pid":101,"address":"0xa","at":[20,20],"size":[900,500],"floating":true},'
                         '{"pid":101,"address":"0xb","at":[600,400],"size":[300,200],"floating":false}]')
    single_clients = '[{"pid":101,"address":"0xa","at":[20,20],"size":[900,500],"floating":true}]'
    rect_function = function(UI, 'xwtab_rect_of')
    for clients, label, wanted in ((single_clients, 'one client', '0xa 20 20 900 500 True\n'),
                                   (duplicate_clients, 'two clients', ''),
                                   ('[]', 'no client', '')):
        result = shell(rect_function + f"\nhyprctl() {{ printf '%s\\n' '{clients}'; }}\nxwtab_rect_of 101\n")
        count = {'one client': 1, 'two clients': 2, 'no client': 0}[label]
        check('rectangle of a pid carried by ' + label,
              result.stdout == wanted and (result.returncode == 0) == bool(wanted)
              and (count == 1 or f'{count} clients carry pid 101' in result.stderr), result.stdout + result.stderr)
    result = shell(room_helpers + double.split('xwtab_rect_of() {')[0]
                   + f"\nhyprctl() {{ if [[ \"$1\" == clients ]]; then printf '%s\\n' '{duplicate_clients}'; fi; }}\n"
                   + '\nxwtab_make_room 101 202\n')
    check('room fails naming the pid and the client count on a duplicate pid',
          result.returncode != 0 and 'FAIL xwtab: no geometry for 101' in result.stdout
          and '2 clients carry pid 101' in result.stdout + result.stderr, result.stdout + result.stderr)

    # The typed helpers run hyprctl in a command substitution, so the placement state lives in files.
    centered = f'state_dir={shlex.quote(str(scratch / "placement"))}\n' + r'''
mkdir -p "$state_dir"
state_put() { printf '%s\n' "$3" > "$state_dir/$1$2"; }
state_get() { cat "$state_dir/$1$2"; }
state_put x 101 40; state_put x 202 1100
state_put y 101 80; state_put y 202 80
state_put w 101 1000; state_put w 202 1000
state_put h 101 720; state_put h 202 720
xwtab_rect_of() {
    local pid="$1" addr=0xa
    [[ "$pid" != 202 ]] || addr=0xb
    printf '%s %s %s %s %s True\n' "$addr" "$(state_get x "$pid")" "$(state_get y "$pid")" "$(state_get w "$pid")" "$(state_get h "$pid")"
}
hyprctl() {
    local pid=101 coords nx ny
    case "$1" in
        monitors)
            printf '%s\n' '[{"name":"DP-2","x":0,"y":0,"width":2560,"height":1440,"focused":true,"activeWorkspace":{"id":1}}]'
            return 0
            ;;
        clients)
            printf '[]\n'
            return 0
            ;;
        layers)
            printf '%s\n' '{"DP-2":{"levels":{"1":[]}}}'
            return 0
            ;;
    esac
    [[ "$2" != *address:0xb* ]] || pid=202
    [[ "$2" != *window.float* ]] || { printf 'ok\n'; return 0; }
    # Sample input: hl.dsp.window.resize({ x = 1250, y = 690, relative = false, window = "address:0xa" }).
    coords=${2#*x = }
    coords=${coords%%, relative*}
    coords=${coords/, y = / }
    read -r nx ny <<< "$coords"
    if [[ "$2" == *window.resize* ]]; then
        state_put x "$pid" $(($(state_get x "$pid") + ($(state_get w "$pid") - nx) / 2))
        state_put y "$pid" $(($(state_get y "$pid") + ($(state_get h "$pid") - ny) / 2))
        state_put w "$pid" "$nx"
        state_put h "$pid" "$ny"
    else
        state_put x "$pid" "$nx"
        state_put y "$pid" "$ny"
    fi
    printf 'ok\n'
}
'''
    result = shell(room_helpers + double + centered + '\nxwtab_make_room 101 202\nxwtab_rect_of 101\n')
    check('room reaches exact parked rectangle with centered resize', result.returncode == 0 and '0xa 20 20 1250 690 True' in result.stdout, result.stdout + result.stderr)
    result = shell(room_helpers + double + centered + '''
state_put x 101 20
state_put y 101 20
state_put w 101 1250
state_put h 101 690
xwtab_saved='101 0xa 40 80 1000 720 True'
xwtab_restore_place
status=$?
xwtab_rect_of 101
exit "$status"
''')
    check('restore reaches exact saved rectangle with centered resize', result.returncode == 0 and '0xa 40 80 1000 720 True' in result.stdout, result.stdout + result.stderr)

    restore = function(UI, 'xwtab_restore_place')
    log.write_text('')
    for failure_step in ('float-on', 'resize', 'move', 'float-off', 'readback'):
        result = shell(TYPED_LIBRARY + restore + "\nfail_at='" + failure_step + "'\n" + r'''
xwtab_saved='101 0xa 20 20 900 500 False'
flea_process_owned() { return 0; }
xwtab_rect_of() { printf '0xa 40 40 900 500 True\n'; }
hyprctl() {
    local step
    case "$2" in
        *window.float*'action = "on"'*) step=float-on ;;
        *window.float*) step=float-off ;;
        *window.resize*) step=resize ;;
        *window.move*) step=move ;;
    esac
    printf '%s\n' "$step" >> "$call_log"
    [[ "$step" != "$fail_at" ]] || return 1
    printf 'ok\n'
}
xwtab_wait_place() {
    printf 'readback\n' >> "$call_log"
    [[ "$fail_at" != readback ]]
}
xwtab_restore_place
status=$?
printf 'status=%s saved=%s\n' "$status" "$xwtab_saved"
fail_at=none
xwtab_restore_place
printf 'retry=%s saved=%s\n' "$?" "$xwtab_saved"
exit "$status"
''' .replace('xwtab_saved=', "call_log='" + str(log) + "'\nxwtab_saved=", 1))
        calls = log.read_text().splitlines()
        expected = ['float-on', 'resize', 'move', 'float-off', 'readback']
        check('restore attempts every step after ' + failure_step + ' failure', calls[-len(expected):] == expected and calls[:len(expected)] == expected, str(calls))
        check('restore retains failed entry for retry after ' + failure_step, result.returncode != 0 and 'status=1 saved=101 0xa' in result.stdout and 'retry=0 saved=\n' in result.stdout and 'XWTAB restore failed' in result.stderr, result.stdout + result.stderr)
        log.write_text('')

    wait_cancel = function(UI, 'xwtab_wait_cancel')
    start = UI.index('    # The catcher never takes keyboard focus', UI.index('case_xwtab()'))
    end = UI.index('    xwtab_wait_unmapped "$apid"', start)
    escape_leg = UI[start:end]
    escape_double = '''
xwtab_cancel_attempts=30; xwtab_cancel_poll=0.1
xwtab_source=101; apid=101
addr=0xa
fail() { echo "FAIL $*"; exit 1; }
sleep() { :; }
xwtab_release() { released=true; }
xwtab_trace_lines() { [[ "${released:-false}" == true ]] && echo 'TABDRAG drag-finished pid=101 action=0'; }
'''
    escape_failure = 'FAIL xwtab: Escape did not cancel while the button was held'
    result = shell(wait_cancel + escape_double + 'omarchy-drive() { :; }\n' + escape_leg)
    check('no-op Escape cannot pass through later release',
          result.returncode != 0 and escape_failure in result.stdout, result.stdout + result.stderr)
    result = shell(wait_cancel + escape_double + 'omarchy-drive() { return 1; }\n' + escape_leg)
    check('another failure cannot satisfy the Escape leg control',
          result.returncode != 0 and escape_failure not in result.stdout and 'FAIL xwtab: Escape did not reach' in result.stdout,
          result.stdout + result.stderr)
    result = shell(wait_cancel + '''
xwtab_cancel_attempts=30; xwtab_cancel_poll=0.1
xwtab_source=101
fail() { exit 1; }
sleep() { :; }
xwtab_trace_lines() { echo 'TABDRAG drag-finished pid=101 action=0'; }
''' + 'xwtab_wait_cancel')
    check('cancel receipt while held satisfies Escape wait', result.returncode == 0, result.stdout + result.stderr)

    unmap_attempts = int(re.search(r'xwtab_unmap_attempts=(\d+)', UI).group(1))
    unmap_constants = UI[UI.index('xwtab_unmap_attempts='):UI.index('xwtab_wait_unmapped()')]
    unmap_function = function(UI, 'xwtab_wait_unmapped')
    mapped_layers = '{"DP-2":{"levels":{"1":[{"namespace":"flea-tab-tearoff","pid":101}]}}}'
    clear_layers = '{"DP-2":{"levels":{"1":[]}}}'
    mapped_failure = 'Escape left the catcher mapped'
    reads_log = scratch / 'layer-reads'
    for name, hyprctl_body, wanted in (
            ('catcher gone at once', f"printf '%s\\n' '{clear_layers}'", 'passes'),
            ('catcher unmapping after two polls',
             f"printf 'read\\n' >> '{reads_log}'; if (( $(wc -l < '{reads_log}') <= 2 )); then printf '%s\\n' '{mapped_layers}'; else printf '%s\\n' '{clear_layers}'; fi",
             'passes'),
            ('catcher never unmapping', f"printf '%s\\n' '{mapped_layers}'", 'left the catcher mapped'),
            ('hyprctl failing', "printf 'Socket error\\n'; return 1", 'hyprctl layers failed'),
            ('hyprctl answering no JSON', "printf 'not json\\n'", 'layers JSON unreadable')):
        log.write_text('')
        reads_log.write_text('')
        result = shell(unmap_constants + '\n' + unmap_function + f'''
fail() {{ echo "FAIL $*"; exit 1; }}
sleep() {{ printf 'poll\\n' >> '{log}'; }}
hyprctl() {{ {hyprctl_body}; }}
xwtab_wait_unmapped 101
''')
        polls = len(log.read_text().splitlines())
        detail = result.stdout + result.stderr + f' polls={polls}'
        if wanted == 'passes':
            condition = result.returncode == 0 and polls == (2 if 'two polls' in name else 0)
        elif wanted == 'left the catcher mapped':
            condition = result.returncode != 0 and mapped_failure in result.stdout and polls == unmap_attempts
        else:
            condition = result.returncode != 0 and wanted in result.stdout and mapped_failure not in result.stdout and polls == 0
        check('Escape layer wait: ' + name, condition, detail)
    check('Escape leg waits for the catcher layer to unmap after release',
          'xwtab_release || fail "xwtab: pointer release failed"\n    xwtab_wait_unmapped "$apid"\n'
          in UI[UI.index('    # Escape mid-drag over A cancels'):UI.index("    printf 'XWTAB escape ok")])

    enter_constants = UI[UI.index('xwtab_receipt_attempts='):UI.index('xwtab_mark_logs()')]
    enter_function = function(UI, 'xwtab_wait_enter')
    enter_receipt = 'qml: TABDRAG enter-window pid=202 ok=true\n'
    enter_source = scratch / 'enter-source.log'
    enter_target = scratch / 'enter-target.log'
    for mode, trace, wanted in (('require', enter_receipt, 0), ('require', '', 'no enter on 202'),
                                ('observe', '', 'observe'), ('observe', enter_receipt, 'observe'),
                                ('typo', enter_receipt, 'typo')):
        enter_source.write_text('')
        enter_target.write_text(trace)
        result = shell(enter_constants + '\n' + enter_function + '\n' + function(UI, 'xwtab_trace_lines') + f'''
xwtab_source=101
xwtab_logs=('{enter_source}' '{enter_target}')
xwtab_marks=(0 0)
xwtab_receipt_attempts=2
fail() {{ printf 'FAIL %s\\n' "$*"; exit 1; }}
sleep() {{ :; }}
xwtab_wait_enter 202 {mode}
''')
        condition = result.returncode == 0 if wanted == 0 else result.returncode != 0 and wanted in result.stdout
        check(f'enter wait with mode {mode} and {"an enter" if trace else "no enter"}', condition,
              result.stdout + result.stderr)
    callers = [line.split()[-1] for line in UI.splitlines() if line.startswith('    xwtab_drag_to_window "')]
    check('every drag gesture passes a known mode', callers and set(callers) <= {'require', 'catcher', 'own', 'refused'},
          repr(callers))
    drag_start = UI.index('xwtab_drag_to_window() {')
    drag_body = UI[drag_start:UI.index('\n}\n', drag_start)]
    check('the drag helper sends catcher, own and refused to their own waits and only the rest to the enter wait',
          routes_modes(drag_body) and enter_wait_calls(UI) == 1, drag_body[-700:])
    second_calls = ['    [[ -n "$bpid" ]] && xwtab_wait_enter "$bpid" observe', '    xwtab_wait_enter $bpid observe',
                    "    xwtab_wait_enter '202' observe", '    seen=$(xwtab_wait_enter)']
    missed = [call for call in second_calls if enter_wait_calls(UI + '\n' + call + '\n') != 2]
    check('caller control counts a second enter wait however it is written', not missed, repr(missed))
    check('caller control does not count the definition', enter_wait_calls('xwtab_wait_enter() {\n}\n') == 0,
          'the definition counted as a call')
    unrouted = drag_body.replace('    elif [[ "$mode" == refused ]]; then\n        xwtab_wait_refused "$bpid" held\n', '')
    check('routing control refuses a refused gesture that falls through to the enter wait',
          unrouted != drag_body and not routes_modes(unrouted), unrouted[-700:])

    own_test_attempts = 2
    own_source_pid = 101
    own_other_pid = 202
    source_log = scratch / 'flea.log'
    target_log = scratch / 'flea-second.log'
    own_constants = UI[UI.index('xwtab_own_attempts='):UI.index('xwtab_wait_own_enter()')]
    own_helpers = '\n'.join(function(UI, name) for name in
                            ('xwtab_trace_lines', 'xwtab_wait_own_enter'))
    own_double = f'''
xwtab_source={own_source_pid}
xwtab_logs=('{source_log}' '{target_log}')
xwtab_marks=(1 0)
xwtab_own_attempts={own_test_attempts}
fail() {{ printf 'FAIL %s\\n' "$*"; exit 1; }}
sleep() {{ printf '%s\\n' "$1" >> '{log}'; }}
'''
    catcher_receipt = f'qml: TABDRAG catcher-enter pid={own_source_pid} global=501,106\n'
    strip_receipt = f'qml: TABDRAG enter-strip pid={own_source_pid} formats=application/x-flea-tab ok=true\n'
    finished_receipt = f'qml: TABDRAG drag-finished pid={own_source_pid} action=0\n'
    stale_receipt = strip_receipt.rstrip('\n') + ' old=true\n'
    for name, trace, succeeds in (
            ('accepts catcher-enter alone', catcher_receipt, True),
            ('accepts source enter-strip ok=true alone', strip_receipt, True),
            ('refuses source enter-strip ok=false', strip_receipt.replace('ok=true', 'ok=false'), False),
            ('refuses another pid enter-strip', strip_receipt.replace(str(own_source_pid), str(own_other_pid)), False),
            ('refuses a pid prefix enter-strip', strip_receipt.replace(f'pid={own_source_pid}', f'pid={own_source_pid}0'), False),
            ('refuses drag-finished', finished_receipt, False),
            ('refuses drag-finished after catcher-enter', catcher_receipt + finished_receipt, False),
            ('refuses drag-finished after enter-strip', strip_receipt + finished_receipt, False),
            ('refuses drag-finished before return mark', finished_receipt + catcher_receipt, False),
            ('refuses receipts before the press mark', '', False)):
        source_log.write_text(stale_receipt + trace)
        target_log.write_text('')
        log.write_text('')
        marked_trace = stale_receipt + (finished_receipt if name == 'refuses drag-finished before return mark' else '')
        return_mark = len(marked_trace.splitlines())
        result = shell(own_constants + own_helpers + own_double + f'\nxwtab_wait_own_enter {return_mark} 0\n')
        detail = result.stdout + result.stderr
        polls = log.read_text().splitlines()
        if succeeds:
            condition = result.returncode == 0 and not polls
        elif finished_receipt in trace:
            condition = result.returncode != 0 and not polls and 'ended before' in detail
        else:
            condition = (result.returncode != 0 and len(polls) == own_test_attempts
                         and 'TABDRAG catcher-enter' in result.stdout
                         and 'TABDRAG enter-strip ok=true' in result.stdout)
        check('own held wait ' + name, condition, detail + ' polls=' + repr(polls))
        if 'ok=false' in trace:
            check('own held timeout prints the last source trace', trace.rstrip('\n') in result.stderr, detail)

    log.write_text('')
    press_mark = len(stale_receipt.splitlines())
    return_mark = len((stale_receipt + catcher_receipt).splitlines())
    result = shell(own_constants + function(UI, 'xwtab_wait_own_enter') + own_double + f'''
xwtab_trace_lines() {{
    if [[ "${{xwtab_marks[0]}}" == {press_mark} ]]; then
        printf 'TABDRAG drag-start pid=%s index=1\\n' "$xwtab_source"
    else
        printf '%s' {shlex.quote(catcher_receipt + finished_receipt)}
    fi
}}
xwtab_wait_own_enter {return_mark} 0
''')
    check('own held wait refuses a finish arriving between the press and return reads',
          result.returncode != 0 and 'ended before' in result.stdout and not log.read_text(),
          result.stdout + result.stderr)

    own_leg = UI[UI.index('    # Out and back onto the own strip reorders'):UI.index('    # A drop on B\'s listing is refused')]
    own_gesture = next(line for line in own_leg.splitlines() if line.strip().startswith('xwtab_drag_to_window '))
    check('own-return leg uses own held wait', own_gesture.endswith('"$apid" "$apid" own'), own_gesture)
    desktop_leg = UI[UI.index('    # B\'s new tab torn off'):UI.index('    xwtab_wait_outcome tearoff')]
    check('desktop tear-off keeps catcher held wait', '"$bpid" desktop catcher' in desktop_leg)
    gesture_helpers = '\n'.join(function(UI, name) for name in
                               ('xwtab_mark_logs', 'xwtab_wait_start', 'xwtab_wait_catcher',
                                'xwtab_drag_to_window', 'xwtab_release'))
    gesture_constants = UI[UI.index('xwtab_outside_x='):UI.index('xwtab_mark_logs()')]
    for name, receipt, succeeds in (
            ('catcher-enter', catcher_receipt, True),
            ('source enter-strip ok=true', strip_receipt, True),
            ('stale outbound catcher then refused return', strip_receipt.replace('ok=true', 'ok=false'), False),
            ('stale outbound catcher without return receipt', '', False)):
        source_log.write_text(stale_receipt)
        target_log.write_text('')
        log.write_text('')
        result = shell(gesture_constants + own_constants + own_helpers + '\n' + gesture_helpers + own_double + f'''
apid=$xwtab_source
sx=501; sy=106; ox=401; oy=106
fixture_source_rect='40 80 1000 720'
fixture_receipt={shlex.quote(receipt)}
fixture_outbound_receipt={shlex.quote(catcher_receipt)}
# Sample input: 40 80 1000 720.
read -r fixture_wx fixture_wy fixture_ww fixture_wh <<< "$fixture_source_rect"
fixture_outside_x=$((fixture_wx + xwtab_outside_x))
fixture_outside_y=$((fixture_wy + fixture_wh + xwtab_outside_y))
xwtab_button_down=false
xwdrag_geometry() {{ printf '%s\\n' "$fixture_source_rect"; }}
xwdrag_glide() {{
    [[ "$xwtab_button_down" == true ]] || return 0
    if [[ "$1 $2" == "$fixture_outside_x $fixture_outside_y" ]]; then
        printf '%s' "$fixture_outbound_receipt" >> '{source_log}'
    elif [[ "$1 $2" == "$ox $oy" ]]; then
        printf '%s' "$fixture_receipt" >> '{source_log}'
    fi
}}
ydotool() {{
    if [[ "$2" == 0x40 ]]; then
        printf 'TABDRAG drag-start pid=%s index=1\\n' "$apid" >> '{source_log}'
    else
        printf 'release\\n' >> '{log}'
    fi
}}
''' + own_gesture + '\n[[ "$xwtab_button_down" == false ]]\n')
        calls = log.read_text().splitlines()
        if succeeds:
            condition = result.returncode == 0 and calls == ['release']
        else:
            condition = result.returncode != 0 and len(calls) == own_test_attempts and 'neither' in result.stdout
        check('own-return gesture ' + ('accepts ' if succeeds else 'refuses ') + name + ' before release',
              condition, result.stdout + result.stderr + log.read_text())

    listing_leg = UI[UI.index('    # A drop on B\'s listing is refused'):UI.index('    # A drop onto a foreign receiver is refused')]
    check('listing refusal requires cursor and refusal proof', '"$apid" "$bpid" refused' in listing_leg)
    move_leg = UI[UI.index('    # B has one tab'):UI.index('    # B\'s new tab torn off')]
    check('one-tab move requires target enter', '"$apid" "$bpid" require' in move_leg)
    check('foreign refusal requires target delivery', '"$apid" "$recv_pid" require' in UI[UI.index('# A drop onto a foreign receiver is refused'):UI.index("printf 'XWTAB foreign-refused")])

    refusal_helpers = UI[UI.index('xwtab_logs='):UI.index('# The addr and rect')]
    refusal_helpers += '\n' + function(UI, 'xwtab_rect_of')
    refusal_double = r'''
apid=101
bpid=202
aid=first
bid=second
bdir=/fixture/b
esc_before='101 202 '
fixture_source_rect='40 80 1000 720'
fixture_target_clients='[{"pid":202,"address":"0xb","at":[1100,80],"size":[1000,720],"floating":true}]'
fixture_tab_count=2
fixture_tab_point='501 106'
fixture_floor_point='1699 468'
fixture_inside_cursor='1699, 468'
fixture_outside_cursor='2100, 468'
refusal_test_attempts=2
xwtab_refused_attempts=$refusal_test_attempts
fixture_cursor=$fixture_inside_cursor
fail() {
    printf 'FAIL %s\n' "$*"
    exit 1
}
sleep() {
    if [[ "$scenario" == delayed-finish && "$xwtab_button_down" == false ]]; then
        printf 'qml: TABDRAG drag-finished pid=%s action=0\n' "$apid" >> "$flea_log"
    fi
}
settle() {
    :
}
xwtab_key() {
    :
}
xwdrag_focus() {
    :
}
flea_pids() {
    printf '%s\n' "$apid" "$bpid"
}
flea_process_owned() {
    [[ "$1" == "$bpid" ]]
}
xwdrag_qs() {
    case "$2" in
        tabCount) printf '%s\n' "$fixture_tab_count" ;;
        path) printf '%s\n' "$bdir" ;;
    esac
}
xwdrag_geometry() {
    printf '%s\n' "$fixture_source_rect"
}
xwtab_tab_point() {
    printf '%s\n' "$fixture_tab_point"
}
xwdrag_floor_point() {
    printf '%s\n' "$fixture_floor_point"
}
hyprctl() {
    case "$1" in
        cursorpos) printf '%s\n' "$fixture_cursor" ;;
        clients) printf '%s\n' "$fixture_target_clients" ;;
    esac
}
xwdrag_glide() {
    fixture_cursor="$1, $2"
    [[ "$xwtab_button_down" == true && "$1 $2" == "$fixture_floor_point" ]] || return 0
    case "$scenario" in
        outside) fixture_cursor=$fixture_outside_cursor ;;
        early-finish) printf 'qml: TABDRAG drag-finished pid=%s action=0\n' "$apid" >> "$flea_log" ;;
        target-enter) printf 'qml: TABDRAG enter-window pid=%s ok=true\n' "$bpid" >> "$run_root/flea-second.log" ;;
    esac
}
ydotool() {
    if [[ "$2" == 0x40 ]]; then
        printf 'qml: TABDRAG drag-start pid=%s index=1\n' "$apid" >> "$flea_log"
        return 0
    fi
    case "$scenario" in
        target-drop) printf 'qml: TABDRAG drop-window pid=%s empty=false\n' "$bpid" >> "$run_root/flea-second.log" ;;
        late-enter) printf 'qml: TABDRAG enter-strip pid=%s ok=true\n' "$bpid" >> "$run_root/flea-second.log" ;;
        no-finish|delayed-finish) return 0 ;;
        accepted)
            printf 'qml: TABDRAG drag-finished pid=%s action=2\n' "$apid" >> "$flea_log"
            return 0
            ;;
    esac
    printf 'qml: TABDRAG drag-finished pid=%s action=0\n' "$apid" >> "$flea_log"
}
'''
    for scenario, succeeds, diagnostic in (
            ('refused', True, ''),
            ('delayed-finish', True, ''),
            ('outside', False, 'cursor outside'),
            ('early-finish', False, 'before release'),
            ('target-enter', False, 'entered or dropped'),
            ('target-drop', False, 'entered or dropped'),
            ('late-enter', False, 'entered or dropped'),
            ('no-finish', False, 'no refused finish'),
            ('accepted', False, 'action=0')):
        source_log = scratch / 'flea.log'
        target_log = scratch / 'flea-second.log'
        source_log.write_text('qml: TABDRAG drag-finished pid=101 action=0 old=true\n')
        target_log.write_text('qml: TABDRAG enter-window pid=202 old=true\nqml: TABDRAG drop-strip pid=202 old=true\n')
        setup = f"flea_log='{source_log}'\nrun_root='{scratch}'\nscenario='{scenario}'\n"
        result = shell(setup + refusal_helpers + '\n' + refusal_double + '\n' + listing_leg)
        detail = result.stdout + result.stderr
        condition = result.returncode == 0 if succeeds else result.returncode != 0 and diagnostic in detail
        check('listing refusal decision: ' + scenario, condition, detail)

    scan = (ROOT / 'tests/xwtab-scan.sh').read_text()
    blocker_start = scan.index("for layer in '1 desktop-widget'")
    blocker_end = scan.index('# Keep the live shell helpers', blocker_start)
    result = shell(f"scratch='{scratch}'\nrepo='{ROOT}'\n" + '''
fail=0
ok() { printf 'ok %s\\n' "$*"; }
bad() {
    printf 'FAIL %s\\n' "$*"
    fail=$((fail + 1))
}
python3() { return 2; }
''' + scan[blocker_start:blocker_end] + '\nexit "$fail"\n')
    check('blocker assertions reject helper errors instead of empty coverage', result.returncode != 0 and result.stdout.count('scan failed (status=2)') == 4 and 'blocks desktop' not in result.stdout, result.stdout + result.stderr)

    (scratch / 'ui/boot').mkdir(parents=True)
    (scratch / 'tests').symlink_to(ROOT / 'tests', target_is_directory=True)
    (scratch / 'ui/boot/bad.qml').write_text('PanelWindow { Keys.onPressed: function(event) {} Item { focus: true } }')
    boot = (ROOT / 'tests/bootload.sh').read_text()
    validation = boot[boot.index('# Keys on a PanelWindow'):boot.index('output=$(env')]
    result = shell("cd '" + str(scratch) + "'\nfiles=bad.qml\n" + validation)
    check('unrelated focus never exempts PanelWindow Keys', result.returncode != 0, result.stdout + result.stderr)

    (scratch / 'ui/boot/unfocused.qml').write_text('PanelWindow { Item { Keys.onPressed: function(event) {} Item { focus: true } } }')
    result = shell("cd '" + str(scratch) + "'\nfiles=unfocused.qml\n" + validation)
    check('focused child never exempts unfocused Item Keys', result.returncode != 0, result.stdout + result.stderr)

    (scratch / 'ui/boot/good.qml').write_text('PanelWindow { Item { Keys.onPressed: function(event) {} focus: true } }')
    result = shell("cd '" + str(scratch) + "'\nfiles=good.qml\n" + validation)
    check('Keys on their own focused Item remain valid', result.returncode == 0, result.stdout + result.stderr)

    probe = (ROOT / 'tests/probes/layer-drop-bottom.sh').read_text()
    move = function(probe, 'move_to')
    verdict_output = function(probe, 'out') + '\n' + function(probe, 'refuse')
    for cursor_status, cursor in ((1, ''), (1, '40, 80'), (0, ''), (0, '40'),
                                  (0, '40, nope'), (0, '40, 80, 9'), (0, '40.5, 80')):
        log.write_text('')
        result = shell('set -u\n' + move + '\n' + verdict_output + f'''
hyprctl() {{
    printf '%s\\n' '{cursor}'
    return {cursor_status}
}}
ydotool() {{ printf 'motion\\n' >> '{log}'; }}
sleep() {{ :; }}
move_to 40 80
''')
        check('cursor read refuses status=' + str(cursor_status) + ' input=' + repr(cursor),
              result.returncode != 0 and result.stdout.startswith('LAYERDROP FAIL ')
              and not log.read_text() and 'unbound variable' not in result.stderr,
              result.stdout + result.stderr)
    for cursor in ('40, 80', '-40, -80'):
        target = cursor.replace(',', '')
        result = shell('set -u\n' + move + '\n' + verdict_output + f'''
hyprctl() {{ printf '%s\\n' '{cursor}'; }}
ydotool() {{ exit 2; }}
move_to {target}
''')
        check('valid integer cursor read reaches target ' + target,
              result.returncode == 0 and not result.stdout, result.stdout + result.stderr)

    for marker, proof, output in (('LAYERDROP PASS', 'panel', '    out "PASS"'),
                                  ('LAYERDROP CATCHER-TEAROFF', 'folder', '    out "CATCHER-TEAROFF"'),
                                  ('LAYERDROP FAIL <why>', 'failure', '    out "FAIL $*"')):
        lines = probe.splitlines()
        previous = lines[lines.index(output) - 1] if output in lines else ''
        check('probe output documents ' + marker + ' and its proof',
              previous.lstrip().startswith('# ') and marker in previous and proof in previous.lower())

    comments = {
        'tests/bootload.sh': ('# A declarative Loader', '# A note is'),
        'tests/dragwire.sh': ('# The Move-alone advertiser',),
        'tests/probes/layer-drop-bottom.sh': ('# A minimal Bottom-layer panel',),
    }
    for relative, starts in comments.items():
        lines = (ROOT / relative).read_text().splitlines()
        for start in starts:
            index = next(index for index, line in enumerate(lines) if line.startswith(start))
            check(relative + ' comment occupies one line: ' + start,
                  not lines[index + 1].startswith('#'))
    for relative in ('tests/probes/layer-drop-bottom.sh',
                     'tests/probes/layer-drop-verdict.sh', 'tests/xwtab-scan.sh'):
        lines = (ROOT / relative).read_text().splitlines()[1:]
        comments = lines[:next(index for index, line in enumerate(lines)
                               if not line.startswith('#'))]
        check(relative + ' header has exactly one complete comment line',
              len(comments) == 1 and comments[0].endswith('.'), repr(comments))

    for suffix, value in (('outside_x', '200'), ('outside_y', '60'), ('target_nudge', '6')):
        name = 'xwtab_' + suffix
        check('tab-drag names ' + name + ' beside helpers',
              name + '=' + value in UI[UI.index('xwtab_logs='):UI.index('xwtab_drag_to_window()')]
              and 'layerdrop_' + suffix + '=' + value in probe)
        shared = function(UI, 'xwtab_drag_to_window')
        escape = UI[UI.index('    # Escape mid-drag over A cancels'):UI.index('    xwtab_wait_cancel')]
        check('shared and Escape gestures use ' + name,
              name in shared and name in escape)

    gesture_start = probe.index('move_to "$sx" "$sy"\ndrag_mark=')
    gesture_end = probe.index('# Wait for an observed panel receipt', gesture_start)
    gesture = probe[gesture_start:gesture_end]
    waits = function(probe, 'layerdrop_wait_drag')
    log.write_text('')
    result = shell(waits + f"\nwork='{scratch}'\ncall_log='{log}'\n" + r'''
sx=348
sy=81
wx=40
wy=40
ww=900
wh=500
dx=8
dy=1432
flea_pid=101
srcdir=/fixture
layerdrop_outside_x=200
layerdrop_outside_y=60
layerdrop_target_nudge=6
layerdrop_drag_attempts=40
layerdrop_drag_poll=0.1
layerdrop_button_down=false
pressed=false
motions=0
refuse() {
    echo "FAIL $*"
    exit 1
}
sleep() { :; }
hyprctl() { printf '%s\n' '{"DP-2": {"levels": {"1": [{"namespace": "flea-tab-tearoff"}]}}}'; }
move_to() {
    printf 'motion %s %s\n' "$1" "$2" >> "$call_log"
    [[ "$pressed" == true ]] || return 0
    motions=$((motions + 1))
    if [[ "$motions" == 1 ]]; then
        printf 'TABDRAG drag-start pid=101 path=/fixture mime=application/x-flea-tab\n' >> "$work/flea.log"
    else
        printf 'TABDRAG catcher-enter pid=101 global=%s,%s\n' "$1" "$2" >> "$work/flea.log"
    fi
}
ydotool() {
    if [[ "$2" == 0x40 ]]; then
        pressed=true
        printf 'press\n' >> "$call_log"
    else
        printf 'release\n' >> "$call_log"
        grep -q catcher-enter "$work/flea.log" || return 1
    fi
}
''' + gesture)
    expected = ['motion 348 81', 'press', 'motion 240 600', 'motion 8 1432', 'motion 14 1432', 'motion 8 1432', 'release']
    check('layer probe waits for receiver after platform start before release', result.returncode == 0 and log.read_text().splitlines() == expected, result.stdout + result.stderr + log.read_text())
    for trace in ('TABDRAG drag-start pid=101 path=/fixture mime=application/x-flea-tab\nTABDRAG drag-finished pid=101 action=0\n', 'TABDRAG drag-start pid=101 path=/fixture mime=application/x-flea-tab\n'):
        (scratch / 'flea.log').write_text(trace)
        result = shell(waits + f"\nwork='{scratch}'\n" + '''
flea_pid=101
srcdir=/fixture
drag_mark=0
log=/dev/null
layerdrop_drag_attempts=2
layerdrop_drag_poll=0.1
refuse() {
    echo "FAIL $*"
    exit 1
}
sleep() { :; }
layerdrop_wait_drag receiver
''')
        check('layer probe refuses release without a held receiver receipt', result.returncode != 0, result.stdout + result.stderr)
    block = probe[probe.index('paths_tsv=""'):]
    check('catcher outcome never masquerades as fixture PANEL-DROP PASS', 'out "PASS"' not in block)
    check('probe calls the exercised catcher verdict',
          'if layerdrop_catcher_hit "$torn" "$lifted_path"; then' in block)
    for torn_path, wanted in (('/fixture', 'LAYERDROP CATCHER-TEAROFF'),
                              ('/elsewhere', 'LAYERDROP FAIL ')):
        result = shell('set -u\n' + verdict_output + f'''
. '{ROOT / 'tests/probes/layer-drop-verdict.sh'}'
layerdrop_path_attempts=2
layerdrop_path_poll=0
flea_pid=101
torn=202
lifted_path=/fixture
before_flea=101
after_flea='101 202'
log=/dev/null
layerdrop_qsid() {{
    case "$1" in
        101) printf 'source-id\\n' ;;
        202) printf 'torn-id\\n' ;;
        *) return 1 ;;
    esac
}}
qs() {{
    case "$3" in
        source-id) printf '/fixture\\n' ;;
        torn-id) printf '%s\\n' '{torn_path}' ;;
        *) return 1 ;;
    esac
}}
sleep() {{ :; }}
''' + block)
        check('live catcher verdict reads torn pid on ' + torn_path,
              result.stdout.startswith(wanted)
              and (result.returncode == 0) == (torn_path == '/fixture'),
              result.stdout + result.stderr)

# Static shape of the branch's test code: comment runs, parser samples, named bounds.
check('receiver constant control refuses a trailing comment on either constant',
      len(receiver_placement('xwtab_receiver_x=1100 # note\nxwtab_receiver_y=500\n')) != RECEIVER_CONSTANT_COUNT
      and len(receiver_placement('xwtab_receiver_x=1100\nxwtab_receiver_y=500 # note\n')) != RECEIVER_CONSTANT_COUNT)
check('receiver constant control accepts the two bare assignments',
      len(receiver_placement('xwtab_receiver_x=1100\nxwtab_receiver_y=500\n')) == RECEIVER_CONSTANT_COUNT)
check('comment run control finds a stacked pair', comment_runs('# one\n# two\ncode\n# three') == [(1, 2)])
check('comment run control skips a shebang and a lone comment', comment_runs('#!/bin/bash\n# one\ncode\n') == [])
for name in STACKED_COMMENT_FILES:
    check(name + ' keeps every comment to one line', comment_runs((ROOT / name).read_text()) == [],
          str(comment_runs((ROOT / name).read_text())))

def dragwire_header_holds(text):
    header = header_comments(text)
    return len(header) == DRAGWIRE_HEADER_LINES and DRAGWIRE_HEADER_TOPIC in header[0]


stacked_header = '#!/bin/bash\n# Guards what a drop target sees.\n# A tab drag offers Move alone.\nset -u\n'
check('dragwire header control refuses a stacked header naming the tab drag', not dragwire_header_holds(stacked_header))
check('dragwire header control refuses one line that never names the tab drag',
      not dragwire_header_holds('#!/bin/bash\n# Guards what a drop target sees.\nset -u\n'))
check('dragwire header control accepts one line naming the tab drag',
      dragwire_header_holds('#!/bin/bash\n# Guards a tab drag, among others.\nset -u\n'))
dragwire_text = (ROOT / 'tests/dragwire.sh').read_text()
check('dragwire header is one comment line that names the tab drag', dragwire_header_holds(dragwire_text),
      repr(header_comments(dragwire_text)))

tearoff = (ROOT / 'ui/boot/tabtearoff.qml').read_text().split('\n')
tearoff_header = [line for line in tearoff[:tearoff.index('Item {')] if line.startswith('//')]
check('tear-off catcher header states its lifecycle in two lines',
      len(tearoff_header) == TEAROFF_HEADER_LINES
      and 'alive only while' in tearoff_header[0] + tearoff_header[1]
      and 'unloads it on every end path' in tearoff_header[1],
      str(tearoff_header))

catcher_start = scan.index(': > "$scratch/catcher-drag.out"')
catcher_note = scan[scan.rindex('\n', 0, scan.rindex('\n', 0, catcher_start)) + 1:catcher_start].strip()
catcher_check = scan[catcher_start:scan.index('# A failure after pressing', catcher_start)]
check('catcher drag check names the mode it drives', 'catcher mode' in catcher_note
      and 'tests/xwtab-safety.py' in catcher_note and 'own-strip' not in catcher_check
      and 'ok "catcher drag' in catcher_check and 'bad "catcher drag' in catcher_check,
      catcher_note + catcher_check)

unsampled_fixture = "x=$(python3 -c '\nprint(json.load(f))\n')"
check('parser sample control flags an unsampled parser', unsampled_parsers(unsampled_fixture) == [2])
check('parser sample control accepts a sample above the parser',
      unsampled_parsers('# Sample input: [].\n' + unsampled_fixture) == [])
check('parser sample control accepts a sample inside the parser',
      unsampled_parsers(unsampled_fixture.replace('print', '# Sample input: [].\nprint')) == [])
check('bare bound control flags a literal seq and placement',
      bare_bounds('for _ in $(seq 1 40); do\n' + LUA_PREFIX + 'window.move({ x = 40, y = 1 })\nx="40 40 900 500 True"') == [1, 2, 3])
check('bare bound control flags a literal helper coordinate and extent',
      bare_bounds('hypr_window_move_absolute "$addr" 40 "$py"\nhypr_window_resize "$addr" "$pw" 480\n'
                  'hypr_window_move "$addr" -20 "$py"') == [1, 2, 3])
check('bare bound control accepts named helper arguments',
      bare_bounds('hypr_window_move_absolute "$addr" "$px" "$py"\nhypr_window_resize_absolute "$addr" "$w" "$h"\n'
                  'hypr_window_focus "$addr"') == [])
check('bare bound control accepts a named bound and a comment',
      bare_bounds('for _ in $(seq 1 "$n"); do\n' + LUA_PREFIX + 'window.move({ x = $px, y = $py })\n# seq 1 40') == [])
probe_text = (ROOT / 'tests/probes/layer-drop-bottom.sh').read_text()
ui_branch = [UI[UI.index('# Sample input: {"pid":101,"address":"0xa","class":"com.thisisgm.flea"'):UI.index('xwdrag_geometry() {')],
             UI[UI.index('xwtab_logs=('):UI.index('# The cursor parks on row 0 above the card')]]
for label, text in (('probe', probe_text), ('ui.sh helpers', ui_branch[0]), ('ui.sh xwtab case', ui_branch[1])):
    check(label + ' puts a sample input above every parser', unsampled_parsers(text) == [], str(unsampled_parsers(text)))
    check(label + ' names its poll bounds and placements', bare_bounds(text) == [], str(bare_bounds(text)))

check('bare timeout control flags a literal', BARE_SUBPROCESS_TIMEOUT.search('run(x, ' + 'timeout=' + '10)') is not None)
check('bare interval control flags a literal', BARE_TIMER_INTERVAL.search('interval: ' + '800') is not None)
check('xwtab-safety.py names its subprocess bound', BARE_SUBPROCESS_TIMEOUT.search(pathlib.Path(__file__).read_text()) is None)
bootload = (ROOT / 'tests/bootload.qml').read_text()
check('bootload.qml names its timer interval',
      BARE_TIMER_INTERVAL.search(bootload) is None and 'interval: root.checkDelayMs' in bootload)

print(f'{checks} safety checks, {failures} failed')
raise SystemExit(bool(failures))
