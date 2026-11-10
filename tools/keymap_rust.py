"""Rust data emission and validation shared by the keymap generator."""
import hashlib
import re

MODIFIERS = {'none': 0, 'text': 0, 'ctrl': 1, 'shift': 2, 'ctrlshift': 3,
             'alt': 4, 'super': 8, 'supershift': 10, 'superalt': 12}
NAMED_KEYS = {'Backspace', 'Backtab', 'BracketLeft', 'BracketRight', 'Comma', 'Delete',
              'Down', 'End', 'Enter', 'Equal', 'Escape', 'Greater', 'Home', 'Insert',
              'Left', 'Menu', 'Minus', 'PageDown', 'PageUp', 'Period', 'Plus', 'Return',
              'Right', 'Space', 'Tab', 'Underscore', 'Up'}


def label_key(label):
    return 'keys.label.' + hashlib.sha256(label.encode()).hexdigest()[:12] if label else ''


def context_label(row):
    label = row['label']
    if label == row['action']:
        label = re.sub(r'([a-z0-9])([A-Z])', r'\1 \2', label).lower()
    return label[:1].upper() + label[1:]


def catalog_entries(data):
    labels = {row['label'] for row in data['pointer']}
    labels.update(context_label(row) for row in data['preset'] + data.get('sheet_extra', [])
                  if row.get('context', 'listing') != 'all' and 'listing' not in row.get('context', 'listing').split(','))
    arms = {row['arm']: row['action'] for row in data['sequence']}
    for table in ('preset', 'ctrlshift', 'ctrl', 'shift', 'code', 'text', 'sheet'):
        for row in data[table]:
            if arms.get(row['action'], row['action']) not in data['native_labels']:
                labels.add(row.get('label', re.sub(r'([a-z0-9])([A-Z])', r'\1 \2', row['action']).lower()))
    entries = {label_key(label): label for label in sorted(labels) if label}
    entries.update(('keys.action.' + action, label) for action, label in data['native_labels'].items())
    entries.update(('keys.group.' + group, group[:1].upper() + group[1:]) for group in data['sheetgroups'])
    entries.update(('keys.' + row['key'], row['text']) for row in data['native_copy'])
    return entries


def validate(data):
    for table in ('ctrlshift', 'ctrl', 'shift', 'code', 'text', 'preset', 'sheet_extra'):
        for row in data.get(table, []):
            text = table == 'text' or row.get('mods') == 'text'
            key = row.get('char' if table == 'text' else 'key')
            if not isinstance(key, str) or not key or not isinstance(row.get('action'), str):
                raise ValueError(f'incomplete {table} binding: {row}')
            if text:
                if len(key) != 1:
                    raise ValueError(f'invalid text key: {key!r}')
            elif key not in NAMED_KEYS and not re.fullmatch(r'[A-Z0-9]|F(?:[1-9]|[12][0-9]|3[0-5])', key):
                raise ValueError(f'invalid key name: {key}')
    digits = data.get('digits', {})
    if (type(digits.get('from')) is not int or type(digits.get('to')) is not int
            or not 0 <= digits['from'] <= digits['to'] <= 9 or not digits.get('prefix')):
        raise ValueError('digits requires single-digit integer bounds and an action prefix')
    seen = set()
    for row in data.get('sequence', []):
        if (not row.get('arm') or not row.get('action') or row.get('kind') not in ('pair', 'confirm')
                or row['arm'] in seen or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', row['arm'])):
            raise ValueError(f'invalid or duplicate sequence: {row}')
        seen.add(row['arm'])
    if not seen:
        raise ValueError('the sequence table is missing or empty')
    options = data.get('sheetoptions', {})
    if type(options.get('cap_budget')) is not int or options['cap_budget'] < 1:
        raise ValueError('sheetoptions requires a positive cap_budget')
    priorities = data.get('sheetties')
    names = {row.get('name') for row in data.get('preset', [])}
    if (not isinstance(priorities, dict) or set(priorities) - names
            or any(not isinstance(chords, list) or any(not isinstance(key, str) for key in chords)
                   for chords in priorities.values())):
        raise ValueError('sheetties requires preset-scoped chord lists')
    if (not isinstance(options.get('mirrors'), list)
            or any(not isinstance(pair, list) or len(pair) != 2
                   or any(not isinstance(action, str) for action in pair) for pair in options['mirrors'])):
        raise ValueError('sheet mirrors must contain action pairs')
    if not isinstance(data.get('native_labels'), dict) or any(
            not isinstance(value, str) or not value for value in data['native_labels'].values()):
        raise ValueError('native_labels requires nonempty strings')
    copies = data.get('native_copy')
    if (not isinstance(copies, list)
            or any(not isinstance(row.get(field), str) or not row[field] for row in copies for field in ('key', 'text'))
            or len({row['key'] for row in copies}) != len(copies)):
        raise ValueError('native_copy requires unique rows with a nonempty key and text')
    for row in data.get('pointer', []):
        if (row.get('where') not in ('listing', 'column', 'neighbour', 'chrome', 'window', 'rail')
                or row.get('row') not in ('any', 'result', 'renaming', 'dir', 'file', 'parent', 'inert', 'place')
                or not row.get('does') or not isinstance(row.get('label'), str)):
            raise ValueError(f'invalid pointer binding: {row}')
        pointer_parts(row['press'])


def pointer_parts(press):
    parts = press.split()
    taps = 2 if parts[-1] == 'x2' else 1
    if taps == 2:
        parts.pop()
    button = parts.pop() if parts else ''
    if (button not in ('left', 'right', 'middle', 'back', 'forward')
            or len(set(parts)) != len(parts) or any(p not in ('ctrl', 'shift') for p in parts)):
        raise ValueError(f'invalid pointer press: {press}')
    return button, sum(MODIFIERS[p] for p in parts), taps


def generate_rust(data, bindings, shared, contexts, extra):
    priorities = data['sheetties']
    unknown = {key for chords in priorities.values() for key in chords} - {row['keys'] for row in bindings + shared}
    if unknown:
        raise ValueError(f'unknown sheet tie-priority spellings: {sorted(unknown)}')
    arms = {row['arm']: row['action'] for row in data['sequence']}
    def display_key(action, label):
        action = arms.get(action, action)
        return 'keys.action.' + action if action in data['native_labels'] else label_key(label)
    def quote(value):
        escapes = {'\\': '\\\\', '"': '\\"', '\n': '\\n', '\r': '\\r', '\t': '\\t'}
        return '"' + ''.join(escapes.get(char, f'\\u{{{ord(char):x}}}' if ord(char) < 32 else char)
                             for char in value) + '"'
    lines = ['// Generated from keys.toml by tools/flea-keymap-gen. Do not edit.',
             'use super::{Binding, Digit, PointerBinding, SequenceBinding, SheetBinding, SheetGroup};']
    def table(name, kind, rows):
        lines.extend(['', '#[rustfmt::skip]', f'pub static {name}: &[{kind}] = &['])
        lines.extend('    ' + row + ',' for row in rows)
        lines.append('];')
    for name, rows in [('PRESET_KEYS', bindings), ('SHARED_KEYS', shared), ('SHEET_EXTRA', extra)]:
        def binding(row):
            label = row.get('label', re.sub(r'([a-z0-9])([A-Z])', r'\1 \2', row['action']).lower())
            wording = display_key(row['action'], label)
            if row['context'] != 'all' and 'listing' not in row['context'].split(','):
                wording = label_key(context_label(row))
            values = [quote(row[k]) for k in ('name', 'mods', 'key', 'keys', 'action', 'context', 'frontend')]
            scopes = row['context'].split(',')
            context_mask = 255 if row['context'] == 'all' else sum(1 << index for index, name in enumerate(contexts[:-1]) if name in scopes)
            values.extend([str(MODIFIERS[row['mods']]), str(context_mask), quote(wording), str(row.get('sheet', True)).lower()])
            return 'Binding::new(' + ', '.join(values) + ')'
        table(name, 'Binding', [binding(row) for row in rows])
    table('POINTER', 'PointerBinding', [
        'PointerBinding::new(' + ', '.join([quote(row['where']), quote(row['press']), quote(row['row']),
            quote(row['does']), quote(label_key(row['label'])), quote(pointer_parts(row['press'])[0]),
            str(pointer_parts(row['press'])[1]), str(pointer_parts(row['press'])[2])]) + ')'
        for row in data['pointer']])
    table('BASE_SHEET', 'SheetBinding', [
        'SheetBinding { action: ' + quote(row['action']) + ', keys: ' + quote(row['keys'])
        + ', label_key: ' + quote(display_key(row['action'], row['label'])) + ' }' for row in data['sheet']])
    table('SHEET_GROUPS', 'SheetGroup', [
        'SheetGroup { id: ' + quote(group) + ', label_key: ' + quote('keys.group.' + group)
        + ', actions: &[' + ', '.join(map(quote, actions)) + '] }'
        for group, actions in data['sheetgroups'].items()])
    table('SEQUENCES', 'SequenceBinding', [
        'SequenceBinding { arm: ' + quote(row['arm']) + ', action: ' + quote(row['action'])
        + ', paired: ' + str(row['kind'] == 'pair').lower() + ' }' for row in data['sequence']])
    table('DIGITS', 'Digit', [
        'Digit { text: ' + quote(str(digit)) + ', action: ' + quote(data['digits']['prefix'] + str(digit)) + ' }'
        for digit in range(data['digits']['from'], data['digits']['to'] + 1)])
    lines.extend(['', 'pub const SHEET_CAP_BUDGET: usize = ' + str(data['sheetoptions']['cap_budget']) + ';'])
    table('SHEET_MIRRORS', '[&str; 2]', ['[' + ', '.join(map(quote, pair)) + ']'
        for pair in data['sheetoptions']['mirrors']])
    table('SHEET_TIE_PRIORITY', '(&str, &[&str])', [
        '(' + quote(name) + ', &[' + ', '.join(map(quote, chords)) + '])'
        for name, chords in priorities.items()])
    return '\n'.join(lines)
