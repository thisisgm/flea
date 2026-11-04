"""Passive event probes never initiate detection with a relist or another query."""
import time
import uuid

from fixtures import checked_mutation


def event_probe(engine, rows, context, root, directory, timeout=3):
    details = engine.ready or {}
    supported = (engine.name == 'flea' and details.get('watch_coverage') == 'recursive'
                 and details.get('watch_healthy') is True and details.get('event_results') is True)
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
    last_epoch = -1
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
            if visible and answer.get('epoch') is not None:
                if answer['epoch'] <= last_epoch:
                    rows.invalidate(context, 'stale mutation epoch')
                    visible = False
                last_epoch = answer['epoch']
            if visible:
                for stamp, clock in [('applied_ns', 'apply_us'), ('pushed_ns', 'push_us')]:
                    native = answer.get(stamp)
                    if native is not None:
                        if not at <= native <= time.perf_counter_ns():
                            rows.invalidate(context, 'native mutation stamp outside observation interval')
                            visible = False
                        else:
                            answer[clock] = (native - at) / 1000
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
