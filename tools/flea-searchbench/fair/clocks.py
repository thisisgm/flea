"""Passive event probes never initiate detection with a relist or another query."""
import time
import uuid
import hashlib
import json
import pathlib
import os
import re
import socket

from eligibility import checked_mutation, marked, read_json


class ColdGateError(RuntimeError):
    pass


def watch_health(details, location):
    if details.get('watch_healthy') is False or details.get('watch_failed') or details.get('watch_overflow'):
        raise ValueError('failed watcher')
    return (details.get('watch_healthy') is True and details.get('event_results') is True
            and (location == 'root' or details.get('watch_coverage') == 'recursive'))


def mutation_clocks(answer, generation, epoch, mutated_ns, observed_ns):
    if answer.get('gen') != generation:
        raise ValueError('stale mutation generation')
    if type(answer.get('epoch')) is not int or answer['epoch'] <= epoch:
        raise ValueError('stale or missing mutation epoch')
    applied, pushed = answer.get('applied_ns'), answer.get('pushed_ns')
    if type(applied) is not int or type(pushed) is not int or not mutated_ns <= applied <= pushed <= observed_ns:
        raise ValueError('mutation clocks outside monotonic interval')
    return dict(apply_us=(applied - mutated_ns) / 1000, push_us=(pushed - mutated_ns) / 1000,
                observed_us=(observed_ns - mutated_ns) / 1000, epoch=answer['epoch'])


def cold_request(root, context):
    root = marked(root)
    identifier = uuid.uuid4().hex
    request = root / ('cache-request-' + identifier + '.json')
    payload = dict(action='drop-page-cache', mode='cold', id=identifier, host=socket.gethostname(),
                   context=context, requested_wall_ns=time.time_ns(), requested_ns=time.monotonic_ns())
    staged = root / ('.' + request.name + '.pending')
    if request.exists() or request.is_symlink():
        raise FileExistsError(request)
    out = staged.open('x')
    try:
        with out:
            json.dump(payload, out, sort_keys=True)
        if request.exists() or request.is_symlink():
            raise FileExistsError(request)
        os.link(staged, request)
    finally:
        checked_mutation(root, staged).unlink(missing_ok=True)
    return request


def validate_cold_response(request, response, now_ns=None):
    root = marked(pathlib.Path(request).parent)
    request, response = pathlib.Path(request), pathlib.Path(response)
    if response.parent != root or response.name != request.name.replace('cache-request-', 'cache-response-', 1):
        raise ColdGateError('cold response path differs')
    payload, receipt = read_json(request, 65536), read_json(response, 65536)
    expected = hashlib.sha256(request.read_bytes()).hexdigest()
    now_ns = time.monotonic_ns() if now_ns is None else now_ns
    if receipt.get('id') != payload['id'] or receipt.get('request_sha256') != expected:
        raise ColdGateError('cold response request identity differs')
    if receipt.get('host') != payload['host'] or receipt.get('context') != payload['context']:
        raise ColdGateError('cold response host/context differs')
    if type(receipt.get('exit_code')) is not int or receipt['exit_code'] != 0:
        raise ColdGateError('cold cache clear failed')
    completed = receipt.get('completed_ns')
    if type(completed) is not int or not payload['requested_ns'] <= completed <= now_ns:
        raise ColdGateError('stale cold response timestamp')
    if now_ns - completed > 600_000_000_000:
        raise ColdGateError('expired cold response')
    return receipt


def cache_gate(output, mode, context):
    if mode == 'warm':
        return
    request = cold_request(output, context)
    response = request.with_name(request.name.replace('cache-request-', 'cache-response-', 1))
    print('COLD_GATE pending: ' + str(request), flush=True)
    deadline = time.monotonic() + 600
    while not response.exists() and not response.is_symlink():
        if time.monotonic() >= deadline:
            raise ColdGateError('cold controller acknowledgement timed out')
        time.sleep(.02)
    try:
        validate_cold_response(request, response)
    except (OSError, ValueError) as error:
        raise ColdGateError('unsafe cold response: ' + str(error)) from error


def event_probe(engine, rows, context, root, directory, timeout=3):
    details = engine.ready or {}
    supported = watch_health(details, 'root' if directory == engine.tree else 'deep')
    if not supported:
        for action in ['create', 'rename', 'delete']:
            rows.row(context, 'event_' + action + '_observed_us', None, 'us', 'unsupported',
                     'requires advertised healthy recursive coverage and standing-query result push')
        return
    token = 'zz_event_' + uuid.uuid4().hex
    old = checked_mutation(root, directory / (token + '_old.txt'))
    new = checked_mutation(root, directory / (token + '_new.txt'))
    engine.query(token)
    gen = engine.generation
    visible = False
    last_epoch = details.get('epoch', -1)
    owned = None
    try:
        for action in ['create', 'rename', 'delete']:
            at = time.perf_counter_ns()
            if action == 'create':
                with checked_mutation(root, old).open('xb'):
                    pass
                owned = old
                expected = str(old.relative_to(engine.tree))
            elif action == 'rename':
                if not visible:
                    rows.invalidate(context, 'event rename lacked prior visibility')
                    return
                checked_mutation(root, old)
                if checked_mutation(root, new).exists():
                    raise RuntimeError('event rename destination exists')
                old.rename(new)
                owned = new
                expected = str(new.relative_to(engine.tree))
            else:
                if not visible:
                    rows.invalidate(context, 'event delete lacked prior visibility')
                    return
                checked_mutation(root, new).unlink()
                owned = None
                expected = None
            forbidden = str((new if action == 'create' else old if action == 'rename' else new).relative_to(engine.tree))
            deadline = time.monotonic() + timeout
            answer = None
            while time.monotonic() < deadline:
                try:
                    answer = engine.finish_query(gen, at, timeout=max(0.001, deadline - time.monotonic()))
                except TimeoutError:
                    break
                if (expected is None or expected in answer['paths']) and forbidden not in answer['paths']:
                    break
                answer = None
            visible = answer is not None
            if visible:
                answer.update(mutation_clocks(answer, gen, last_epoch, at, time.perf_counter_ns()))
                last_epoch = answer['epoch']
            rows.row(context, 'event_' + action + '_observed_us',
                     answer['full_ms'] * 1000 if visible else None, 'us', 'ok' if visible else 'quality-fail',
                     'passive standing query; raw mutation-to-observation; no query or relist after mutation')
            for clock in ['apply_us', 'push_us', 'epoch']:
                rows.row(context, 'event_' + action + '_' + clock, answer.get(clock) if answer else None,
                         'epoch' if clock == 'epoch' else 'us',
                         'ok' if answer and answer.get(clock) is not None else 'unsupported',
                         'native clock must share the mutation clock domain; no inferred index-apply stamp')
            if not visible:
                rows.invalidate(context, 'event-only mutation not reflected')
                return
    finally:
        if owned is not None:
            checked_mutation(root, owned).unlink(missing_ok=True)


def cancellation_probe(engine, rows, context, text):
    if engine.name != 'flea':
        rows.row(context, 'supersession_current_result_us', None, 'us', 'unsupported')
        rows.row(context, 'cancel_to_close_ack_us', None, 'us', 'unsupported')
        return
    before = engine.stale_replies
    for typed in [text[:1], text[:3], text[:2], text]:
        at = time.perf_counter_ns()
        gen = engine.begin_query(typed)
        time.sleep(0.001)
    result = engine.finish_query(gen, at)
    rows.row(context, 'supersession_current_result_us', result['full_ms'] * 1000, 'us')
    rows.row(context, 'stale_replies_rejected', engine.stale_replies - before, 'replies')
    at = time.perf_counter_ns()
    engine.send(dict(c='searchcancel'))
    engine.send(dict(c='searchclose'))
    _, stamp = engine.until(lambda obj: obj.get('t') == 'searchclosed', timeout=3)
    rows.row(context, 'cancel_to_close_ack_us', (stamp - at) / 1000, 'us', note='acknowledged session close, not proof of worker idle')
    rows.row(context, 'cancel_to_idle_us', None, 'us', 'unsupported', 'no shipping worker-idle counter')


def build_cancel_probe(engine, rows, context, text):
    engine.begin_open()
    opened = time.perf_counter_ns()
    engine.begin_query(text[:1])
    time.sleep(0.001)
    engine.begin_query(text[:3])
    at = time.perf_counter_ns()
    engine.send(dict(c='searchcancel'))
    engine.send(dict(c='searchclose'))
    _, stamp = engine.until(lambda obj: obj.get('t') == 'searchclosed', timeout=3)
    rows.row(context, 'queued_build_cancel_to_close_ack_us', (stamp - at) / 1000, 'us',
             note='queued after open; fast fixtures may already be built; close ack is not worker idle')
    rows.row(context, 'cancel_enqueued_after_open_us', (at - opened) / 1000, 'us')
    rows.row(context, 'cancel_to_idle_us', None, 'us', 'unsupported', 'requires native worker-idle stamp')


def storage_evidence(root, storage, mountinfo=None, sys_root=pathlib.Path('/sys')):
    root = marked(root)
    def unescape(value):
        return re.sub(r'\\([0-7]{3})', lambda m: chr(int(m[1], 8)), value)
    mounts = []
    for line in (mountinfo if mountinfo is not None else pathlib.Path('/proc/self/mountinfo').read_text()).splitlines():
        fields = line.split()
        split = fields.index('-')
        point = pathlib.Path(unescape(fields[4]))
        if root == point or point in root.parents:
            mounts.append((point, fields[2], fields[split + 1], unescape(fields[split + 2])))
    if not mounts:
        raise ValueError('fixture mount cannot be established')
    point, device, filesystem, source = max(mounts, key=lambda item: len(item[0].parts))
    network = filesystem in {'nfs', 'nfs4', 'cifs', 'smb3', 'fuse.sshfs'}
    device_path = str((sys_root / 'dev/block' / device).resolve())
    usb = bool(re.search(r'/usb[0-9]+/', device_path))
    actual = 'nas' if network else 'usb' if usb else 'local'
    if storage != actual:
        raise ValueError('wrong storage: requested ' + storage + ', observed ' + actual)
    return dict(storage=actual, mount=str(point), device=device, filesystem=filesystem,
                source=source, sys_device=device_path, st_dev=root.stat().st_dev)


