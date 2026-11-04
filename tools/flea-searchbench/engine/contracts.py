"""Eligibility is applied after collection, so later failures invalidate earlier timings."""
import hashlib
import os
import stat


POLICY = 'visible; no ignores; no directory-symlink descent; root excluded'


def identities(tree):
    root = os.fsencode(tree)
    pending, result = [root], []
    while pending:
        directory = pending.pop()
        with os.scandir(directory) as entries:
            for entry in entries:
                if entry.name.startswith(b'.'):
                    continue
                kind = 'd' if entry.is_dir(follow_symlinks=False) else 'f'
                result.append((os.path.relpath(entry.path, root), kind))
                if kind == 'd':
                    pending.append(entry.path)
    return sorted(result)


def digest(items):
    value = hashlib.sha256()
    for path, kind in sorted(items):
        raw = os.fsencode(path)
        value.update(len(raw).to_bytes(8, 'little'))
        value.update(raw)
        value.update(kind.encode('ascii'))
    return value.hexdigest()


def inventory_report(expected, observed, details):
    reasons = []
    if details.get('capped'):
        reasons.append('capped index')
    if details.get('partial') or details.get('complete') is False:
        reasons.append('partial index: ' + str(details.get('reason', 'unspecified')))
    if len(observed) != len(set(observed)):
        reasons.append('duplicate raw identities')
    if sorted(observed) != sorted(expected):
        reasons.append('raw identity inventory differs')
    if details.get('n') != len(expected):
        reasons.append('indexed count differs')
    if details.get('files') is not None and details['files'] != sum(kind == 'f' for _, kind in expected):
        reasons.append('indexed file/type count differs')
    return dict(eligible=not reasons, reasons=reasons, expected=len(expected), observed=len(observed),
                expected_sha256=digest(expected), observed_sha256=digest(observed), policy=POLICY,
                directories=sum(kind == 'd' for _, kind in expected),
                files=sum(kind == 'f' for _, kind in expected))


def classify_rank(paths, target, full_paths=None, complete=False):
    if target in paths:
        return paths.index(target) + 1, 'ok'
    if full_paths is not None:
        return (-1, 'censored') if target in full_paths else (0, 'miss')
    return (0, 'miss') if complete else (-1, 'unresolved')


# The rank among file rows only, so engines that order folders differently compare like for like.
def classify_file_rank(paths, folders, target, full_paths=None, complete=False):
    if target in folders:
        raise ValueError('file rank asked for a folder target: ' + target)
    return classify_rank([path for path in paths if path not in folders], target, full_paths, complete)


def checked_result(result, gen, limit, complete_matches=None):
    if result.get('gen') != gen:
        raise ValueError('stale generation')
    paths = result['paths']
    if result.get('n') is not None and (not isinstance(result['n'], int) or result['n'] < len(paths)):
        raise ValueError('invalid total-match count')
    total = result.get('n') if complete_matches is None else complete_matches
    if complete_matches is not None and result.get('n') is not None and result['n'] != complete_matches:
        raise ValueError('reported total differs from complete audit')
    if total is not None and (type(total) is not int or total < 0 or len(paths) != min(limit, total)):
        raise ValueError('returned head does not cover requested matches')
    if len(paths) > limit or len(paths) != len(set(paths)):
        raise ValueError('invalid returned head')
    for name in paths:
        raw = os.fsencode(name)
        if not raw or raw.startswith(b'/') or b'..' in raw.split(b'/') or b'\0' in raw:
            raise ValueError('invalid relative identity')
    return total is not None


def quality_reason(kind, paths, target):
    if target not in paths:
        return 'final target missing'
    if kind in {'exact', 'typo', 'path'} and paths[0] != target:
        return 'unambiguous final target misranked'
    return None


def observed_identities(tree, paths):
    result = []
    for path in paths:
        mode = os.lstat(os.path.join(os.fsencode(tree), os.fsencode(path))).st_mode
        result.append((os.fsencode(path), 'd' if stat.S_ISDIR(mode) else 'f'))
    return result
