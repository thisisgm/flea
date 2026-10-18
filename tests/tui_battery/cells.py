"""Incremental VT cell evidence; graphics strings remain separate from text."""
import codecs
from copy import deepcopy
import re
import unicodedata

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
    def __init__(self, columns=130, rows=31):
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
        return dict(ch=" ", width=1, **self.style)

    def resize(self, columns, rows):
        self.columns, self.rows = columns, rows
        self.grid = [[self.blank() for _ in range(columns)] for _ in range(rows)]
        self.x = self.y = 0

    def feed(self, data):
        s = self.pending + self.decoder.decode(data)
        pos = 0
        csi_pattern = re.compile(r"\x1b\[([0-?]*)([ -/]*)([@-~])")
        while pos < len(s):
            if s.startswith("\x1b[", pos):
                match = csi_pattern.match(s, pos)
                if not match:
                    break
                self.csi(match[1], match[3])
                pos = match.end()
            elif s[pos:pos + 2] in ("\x1b_", "\x1bP", "\x1b]", "\x1b^"):
                end = s.find("\x1b\\", pos + 2)
                bell = s.find("\x07", pos + 2) if s.startswith("\x1b]", pos) else -1
                stop = min([v for v in (end, bell) if v >= 0], default=-1)
                if stop < 0:
                    if len(s) - pos > 8 * 1024 * 1024:
                        raise ValueError("terminal string exceeds evidence budget")
                    break
                consumed = stop + (1 if stop == bell else 2)
                self.graphics.append(s[pos:consumed])
                pos = consumed
            elif s.startswith("\x1b", pos):
                if len(s) - pos < 2:
                    break
                if s[pos + 1] == "7":
                    self.saved = (self.x, self.y)
                elif s[pos + 1] == "8":
                    self.x, self.y = self.saved
                else:
                    self.unsupported.add(repr(s[pos:pos + 2]))
                pos += 2
            else:
                ch = s[pos]
                pos += 1
                if ch == "\r":
                    self.x = 0
                elif ch == "\n":
                    self.y = min(self.rows - 1, self.y + 1)
                elif ch == "\b":
                    self.x = max(0, self.x - 1)
                elif ch == "\t":
                    self.x = min(self.columns - 1, (self.x // 8 + 1) * 8)
                elif ord(ch) >= 32 and ch != "\x7f":
                    w = width(ch)
                    if not w:
                        self.grid[self.y][max(0, self.x - 1)]["ch"] += ch
                        continue
                    if self.x >= self.columns:
                        self.x, self.y = 0, min(self.rows - 1, self.y + 1)
                    self.grid[self.y][self.x] = dict(ch=ch, width=w, **self.style)
                    if w == 2 and self.x + 1 < self.columns:
                        self.grid[self.y][self.x + 1] = dict(ch="", width=0, **self.style)
                    self.x += w
        self.pending = s[pos:]

    def csi(self, params, final):
        private = params.startswith(("?", ">", "<", "="))
        nums = [int(n or 0) for n in params.lstrip("?><=").replace(":", ";").split(";")]
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
        elif final == "m":
            self.sgr(nums)
        elif final == "J":
            if nums[0] in (2, 3):
                self.grid = [[self.blank() for _ in range(self.columns)] for _ in range(self.rows)]
            elif nums[0] == 0:
                for y in range(self.y, self.rows):
                    for x in range(self.x if y == self.y else 0, self.columns):
                        self.grid[y][x] = self.blank()
            else:
                self.unsupported.add(f"CSI {params}{final}")
        elif final == "K":
            start, end = (0, self.columns) if nums[0] == 2 else ((0, self.x + 1) if nums[0] == 1 else (self.x, self.columns))
            for x in range(min(start, self.columns), end):
                self.grid[self.y][x] = self.blank()
        elif final in ("u", "c", "n", "t", "q"):
            pass
        else:
            self.unsupported.add(f"CSI {params}{final}")

    def sgr(self, nums):
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
