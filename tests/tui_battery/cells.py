"""Incremental VT cell evidence; graphics strings remain separate from text."""
import codecs
from copy import deepcopy
import re
import unicodedata

from .geometry import COLUMNS, ROWS

MAX_TERMINAL_STRING_CHARS = 8 * 1024 * 1024
MAX_CONTROL_BYTES = 4096
C0 = r"\x00-\x17\x19\x1c-\x1f"
CSI = re.compile(r"\x1b\[([0-?" + C0 + r"]*)([ -/" + C0 + r"]*)([@-~])")
CSI_PREFIX = re.compile(r"\x1b\[[0-?" + C0 + r"]*[ -/" + C0 + r"]*")
ESCAPE = re.compile(r"\x1b([ -/]*)([0-~])")
CSI_BODY = re.compile(r"[0-9;]*")
SGR_GROUP = re.compile(r"[0-9:]*")

PALETTE = ["#1c120d", "#d65a4f", "#79ab55", "#dc9c36", "#509467", "#957a5a", "#599181", "#c6b395",
           "#8b7b74", "#d65a4f", "#79ab55", "#dc9c36", "#509467", "#957a5a", "#599181", "#d8cdbb"]


def width(ch):
    if unicodedata.combining(ch) or unicodedata.category(ch) in ("Cf", "Mn", "Me"):
        return 0
    return 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1


def color(index):
    if index < 16:
        return PALETTE[index]
    if index >= 232:
        v = 8 + 10 * (index - 232)
        return f"#{v:02x}{v:02x}{v:02x}"
    n = index - 16
    return "#" + "".join(f"{[0, 95, 135, 175, 215, 255][v]:02x}" for v in (n // 36, (n // 6) % 6, n % 6))


class Screen:
    def __init__(self, columns=COLUMNS, rows=ROWS):
        self.columns, self.rows = columns, rows
        self.x = self.y = 0
        self.style = self.defaults()
        self.grid = [[self.blank() for _ in range(columns)] for _ in range(rows)]
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        self.pending = ""
        self.graphics = []
        self.modes = {}
        self.unsupported = set()
        self.saved = (0, 0)
        self.frames = 0

    @staticmethod
    def defaults():
        return dict(fg="#c6b395", bg="#1c120d", bold=False, dim=False, reverse=False, underline=False)

    def blank(self):
        return dict(self.defaults(), ch=" ", width=1, fg=self.style["fg"], bg=self.style["bg"])

    def resize(self, columns, rows):
        self.columns, self.rows = columns, rows
        self.grid = [[self.blank() for _ in range(columns)] for _ in range(rows)]
        self.x = self.y = 0

    # Sample input: b'\x1b[1;1H\x1b[38;2;255;0;0mhello\x1b(B\x1b[m'.
    def feed(self, data):
        s = self.pending + self.decoder.decode(data)
        pos = 0
        while pos < len(s):
            if s.startswith("\x1b[", pos):
                match = CSI.match(s, pos)
                if not match:
                    if len(s) - pos > MAX_CONTROL_BYTES or not CSI_PREFIX.fullmatch(s[pos:]):
                        raise ValueError("unparseable terminal CSI: " + repr(s[pos:pos + MAX_CONTROL_BYTES]))
                    break
                if match.end() - pos > MAX_CONTROL_BYTES:
                    raise ValueError("terminal CSI exceeds evidence budget")
                for ch in match[1] + match[2]:
                    if ord(ch) < 32:
                        self.control(ch)
                self.csi("".join(ch for ch in match[1] if ord(ch) >= 32), match[3])
                pos = match.end()
            elif s[pos:pos + 2] in ("\x1b_", "\x1bP", "\x1b]", "\x1b^"):
                end = s.find("\x1b\\", pos + 2)
                bell = s.find("\x07", pos + 2) if s.startswith("\x1b]", pos) else -1
                stop = min([v for v in (end, bell) if v >= 0], default=-1)
                if stop < 0:
                    if len(s) - pos > MAX_TERMINAL_STRING_CHARS:
                        raise ValueError("terminal string exceeds evidence budget")
                    break
                consumed = stop + (1 if stop == bell else 2)
                self.graphics.append(s[pos:consumed])
                pos = consumed
            elif s.startswith("\x1b", pos):
                match = ESCAPE.match(s, pos)
                if not match:
                    if len(s) - pos > MAX_CONTROL_BYTES or not re.fullmatch(r"\x1b[ -/]*", s[pos:]):
                        raise ValueError("unparseable terminal escape: " + repr(s[pos:pos + MAX_CONTROL_BYTES]))
                    break
                token = match[0]
                if token == "\x1b7":
                    self.saved = (self.x, self.y)
                elif token == "\x1b8":
                    self.x, self.y = self.saved
                elif token != "\x1b(B":
                    self.unsupported.add(repr(token))
                pos = match.end()
            else:
                ch = s[pos]
                pos += 1
                if ord(ch) < 32:
                    self.control(ch)
                elif ord(ch) >= 32 and ch != "\x7f":
                    w = width(ch)
                    if not w:
                        previous = min(self.columns - 1, max(0, self.x - 1))
                        while previous and self.grid[self.y][previous]["width"] == 0:
                            previous -= 1
                        self.grid[self.y][previous]["ch"] += ch
                        continue
                    if w > self.columns:
                        raise ValueError("terminal glyph exceeds the grid width: " + repr(ch))
                    if self.x + w > self.columns:
                        self.x, self.y = 0, min(self.rows - 1, self.y + 1)
                    self.release_partners(w)
                    self.grid[self.y][self.x] = dict(ch=ch, width=w, **self.style)
                    if w == 2 and self.x + 1 < self.columns:
                        self.grid[self.y][self.x + 1] = dict(ch="", width=0, **self.style)
                    self.x += w
        self.pending = s[pos:]

    def control(self, ch):
        if ch == "\r":
            self.x = 0
        elif ch in ("\n", "\v", "\f"):
            self.y = min(self.rows - 1, self.y + 1)
        elif ch == "\b":
            self.x = max(0, self.x - 1)
        elif ch == "\t":
            self.x = min(self.columns - 1, (self.x // 8 + 1) * 8)

    def erase(self, y, start, end):
        # Erasing half of a wide glyph erases the whole glyph, as xterm does.
        row = self.grid[y]
        if start and row[start]["width"] == 0:
            start -= 1
        if end and end < self.columns and row[end - 1]["width"] == 2:
            end += 1
        for x in range(start, end):
            row[x] = self.blank()

    def release_partners(self, w):
        # Printing over half of a wide glyph blanks its partner half, as xterm does.
        row = self.grid[self.y]
        x, last = self.x, self.x + w - 1
        if x and row[x]["width"] == 0:
            row[x - 1] = self.blank()
        if row[last]["width"] == 2 and last + 1 < self.columns:
            row[last + 1] = self.blank()

    # Sample input: params='1;2', final='H'; params='38:2::255:0:0', final='m'.
    def csi(self, params, final):
        marker = params[:1] if params[:1] in ("?", ">", "<", "=") else ""
        body = params[len(marker):]
        if final == "m" and not marker:
            self.sgr(params)
            return
        if final == "m" or not CSI_BODY.fullmatch(body):
            # Private-marker SGR and colon or marker params on other finals stay unmodeled.
            self.unsupported.add(f"CSI {params}{final}")
            return
        private = bool(marker)
        nums = [int(n or 0) for n in body.split(";")]
        first = nums[0] or 1
        if final in ("H", "f"):
            self.y = min(self.rows - 1, max(0, first - 1))
            self.x = min(self.columns - 1, max(0, (nums[1] if len(nums) > 1 else 1) - 1))
            if (self.x, self.y) == (0, 0):
                self.frames += 1
        elif final in "ABCD":
            dx, dy = {"A": (0, -first), "B": (0, first), "C": (first, 0), "D": (-first, 0)}[final]
            self.x = min(self.columns - 1, max(0, self.x + dx))
            self.y = min(self.rows - 1, max(0, self.y + dy))
        elif final == "G":
            self.x = min(self.columns - 1, first - 1)
        elif final == "d":
            self.y = min(self.rows - 1, first - 1)
        elif final in ("h", "l") and private:
            for n in nums:
                self.modes[str(n)] = final == "h"
        elif final == "J":
            # A pending-wrap cursor (x == columns) still sits on the last cell.
            column = min(self.x, self.columns - 1)
            if nums[0] == 2:
                self.grid = [[self.blank() for _ in range(self.columns)] for _ in range(self.rows)]
            elif nums[0] == 0:
                for y in range(self.y, self.rows):
                    self.erase(y, column if y == self.y else 0, self.columns)
            elif nums[0] == 1:
                for y in range(self.y):
                    self.erase(y, 0, self.columns)
                self.erase(self.y, 0, column + 1)
            elif nums[0] != 3:
                # Mode 3 erases saved lines only, so the visible grid keeps its cells.
                self.unsupported.add(f"CSI {params}{final}")
        elif final == "K":
            # A pending-wrap cursor (x == columns) still sits on the last cell.
            column = min(self.x, self.columns - 1)
            start, end = (0, self.columns) if nums[0] == 2 else ((0, column + 1) if nums[0] == 1 else (column, self.columns))
            self.erase(self.y, start, end)
        elif final in ("u", "c", "n", "t", "q"):
            pass
        else:
            self.unsupported.add(f"CSI {params}{final}")

    # Sample input: '1;38;2;255;0;0' or '38:2::255:0:0;48:5:16'.
    def sgr(self, params):
        nums = []
        for group in params.split(";"):
            if not SGR_GROUP.fullmatch(group):
                self.unsupported.add("SGR " + group)
                continue
            if ":" not in group:
                nums.append(int(group or 0))
                continue
            fields = group.split(":")
            channels = fields[2:]
            if fields[:2] in (["38", "2"], ["48", "2"]):
                if len(channels) == 4 and channels[0] in ("", "0"):
                    channels = channels[1:]
                count = 3
            elif fields[:2] in (["38", "5"], ["48", "5"]):
                count = 1
            else:
                self.unsupported.add("SGR " + group)
                continue
            if len(channels) != count or any(not c.isdecimal() or not 0 <= int(c) <= 255 for c in channels):
                self.unsupported.add("SGR " + group)
                continue
            nums.extend([int(fields[0]), int(fields[1]), *map(int, channels)])
        i = 0
        while i < len(nums):
            n = nums[i]
            if n == 0:
                self.style = self.defaults()
            elif n in (1, 2, 4, 7):
                self.style[{1: "bold", 2: "dim", 4: "underline", 7: "reverse"}[n]] = True
            elif n in (22, 24, 27):
                for k in ({22: ("bold", "dim"), 24: ("underline",), 27: ("reverse",)}[n]):
                    self.style[k] = False
            elif n in (39, 49):
                self.style["fg" if n == 39 else "bg"] = self.defaults()["fg" if n == 39 else "bg"]
            elif 30 <= n <= 37 or 90 <= n <= 97:
                self.style["fg"] = color(n - 30 if n < 90 else n - 90 + 8)
            elif 40 <= n <= 47 or 100 <= n <= 107:
                self.style["bg"] = color(n - 40 if n < 100 else n - 100 + 8)
            elif n in (38, 48) and i + 2 < len(nums):
                key = "fg" if n == 38 else "bg"
                if nums[i + 1] == 5 and 0 <= nums[i + 2] <= 255:
                    self.style[key] = color(nums[i + 2])
                    i += 2
                elif nums[i + 1] == 2 and i + 4 < len(nums) and all(0 <= c <= 255 for c in nums[i + 2:i + 5]):
                    self.style[key] = "#" + "".join(f"{v:02x}" for v in nums[i + 2:i + 5])
                    i += 4
                else:
                    self.unsupported.add(f"SGR {nums}")
            else:
                self.unsupported.add(f"SGR {n}")
            i += 1

    def snapshot(self):
        return {"columns": self.columns, "rows": self.rows, "cursor": [self.x, self.y],
                "grid": deepcopy(self.grid), "modes": dict(self.modes), "frames": self.frames,
                "unsupported": sorted(self.unsupported), "pending": bool(self.pending)}


def text(snapshot):
    return "\n".join("".join(c["ch"] for c in row) for row in snapshot["grid"])


def predicate(snapshot, rule):
    content = text(snapshot)
    return (all(s in content for s in rule.get("contains", []))
            and all(s not in content for s in rule.get("absent", []))
            and all(re.search(s, content) for s in rule.get("regex", []))
            and snapshot["frames"] > 0 and not snapshot.get("pending"))
