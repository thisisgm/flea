# A row whose name is not UTF-8 acts on that exact file over the wire, never on its lossy twin.
# argv: <flea binary>. Every write lands inside one mkdtemp root that carries the sandbox marker.
import json, os, queue, shutil, subprocess, sys, tempfile, threading, time

BIN = sys.argv[1]
MARKER = ".flea-test-sandbox"
INVALID = b"col\xff"
TWIN = "col�".encode()
FAILS = []


def check(name, cond, note=""):
    print(("PASS " if cond else "FAIL ") + name + ("" if cond else "  " + note))
    if not cond:
        FAILS.append(name)


class Backend:
    def __init__(self, home):
        env = {"PATH": "/usr/bin:/bin", "HOME": home, "LANG": "C.UTF-8", "LC_ALL": "C.UTF-8"}
        for key, sub in (("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache"),
                         ("XDG_STATE_HOME", "state"), ("XDG_RUNTIME_DIR", "run")):
            env[key] = os.path.join(home, sub)
            os.makedirs(env[key], mode=0o700, exist_ok=True)
        env["FLEA_UNDO_DIR"] = os.path.join(home, "run", "flea")
        self.env = env
        self.p = subprocess.Popen([BIN, "--backend"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.DEVNULL, env=env, cwd=home)
        self.q = queue.Queue()
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        for raw in self.p.stdout:
            try:
                self.q.put(json.loads(raw.decode("utf-8")))
            except ValueError:
                self.q.put({"t": "_unparsed", "raw": raw[:200]})
        self.q.put({"t": "_eof"})

    def send(self, obj):
        self.p.stdin.write(json.dumps(obj).encode() + b"\n")
        self.p.stdin.flush()

    def req(self, obj, pred, timeout=30):
        self.send(obj)
        end = time.time() + timeout
        while time.time() < end:
            try:
                got = self.q.get(timeout=max(0.01, end - time.time()))
            except queue.Empty:
                break
            if got.get("t") == "_eof":
                break
            if pred(got):
                return got
        return None

    def collect(self, obj, last, timeout=30):
        """Every reply up to and including the first that last() accepts."""
        self.send(obj)
        got, end = [], time.time() + timeout
        while time.time() < end:
            try:
                one = self.q.get(timeout=max(0.01, end - time.time()))
            except queue.Empty:
                break
            if one.get("t") == "_eof":
                break
            got.append(one)
            if last(one):
                break
        return got

    def list(self, path):
        return self.req({"c": "list", "path": path, "first": 100, "hidden": True}, lambda o: o.get("t") in ("rows", "error"))

    def undo(self):
        for _ in range(40):
            got = self.req({"c": "undo"}, lambda o: o.get("t") in ("undone", "error"))
            if got and got.get("t") == "undone":
                return got
            time.sleep(0.1)
        return got

    def quit(self):
        try:
            self.send({"c": "quit"})
            self.p.stdin.close()
            self.p.wait(timeout=20)
        except Exception:
            self.p.kill()
            self.p.wait()


def tree(root):
    out = {}
    for name in sorted(os.listdir(os.fsencode(root))):
        path = os.path.join(os.fsencode(root), name)
        if os.path.isdir(path):
            out[name] = ("d", tree(path))
        else:
            with open(path, "rb") as f:
                out[name] = ("f", f.read())
    return out


def row_of(rows, raw):
    """The index of the row whose wire text names the exact bytes raw."""
    for i, row in enumerate(rows):
        if wire_bytes(row["n"]) == raw:
            return i
    return None


def wire_bytes(text):
    """The bytes a wire name text stands for: '<lossy>\\0<hex>' per component, anything else UTF-8."""
    out = []
    for part in text.split("/"):
        if "\0" in part:
            out.append(bytes.fromhex(part.split("\0", 1)[1]))
        else:
            out.append(part.encode())
    return b"/".join(out)


def pair(root, tag):
    src = os.path.join(root, tag, "src")
    dst = os.path.join(root, tag, "dst")
    os.makedirs(src)
    os.makedirs(dst)
    with open(os.path.join(os.fsencode(src), INVALID), "wb") as f:
        f.write(b"I")
    with open(os.path.join(os.fsencode(src), TWIN), "wb") as f:
        f.write(b"VV")
    return src, dst


def listed(k, src):
    r = k.list(src)
    assert r and r.get("t") == "rows", r
    inv = row_of(r["rows"], INVALID)
    if inv is None:
        # A build that spells both rows alike: aim at the row whose stat failed, as a person would see it.
        sizes = [row["s"] for row in r["rows"]]
        inv = sizes.index(0) if 0 in sizes else 0
    return r, inv, row_of(r["rows"], TWIN)


def run(root, home):
    k = Backend(home)
    try:
        src, dst = pair(root, "pair")
        before = tree(src)
        r, inv, twin = listed(k, src)
        check("list keeps the two rows apart", row_of(r["rows"], INVALID) is not None and twin is not None and inv != twin, str(r["rows"]))
        check("list stats the invalid row itself", inv is not None and r["rows"][inv]["s"] == 1, str(r["rows"]))
        lst = r["listing"]

        t = k.req({"c": "trash", "id": 2, "rows": [inv], "listing": lst}, lambda o: o.get("t") in ("trashed", "error"))
        left = tree(src)
        check("trash rows takes the invalid file", t and t.get("ok") == 1 and INVALID not in left and TWIN in left, "%s %r" % (t, left))
        check("trashed names the exact path", t and wire_bytes(t["moved"][0]).endswith(INVALID), str(t))
        k.undo()
        check("trash undo restores it", tree(src) == before, repr(tree(src)))

        r, inv, twin = listed(k, src)
        path = src + "/" + r["rows"][inv]["n"]
        t = k.req({"c": "trash", "id": 3, "paths": [path]}, lambda o: o.get("t") in ("trashed", "error"))
        left = tree(src)
        check("trash paths takes the invalid file", t and t.get("ok") == 1 and INVALID not in left and TWIN in left, "%s %r" % (t, left))
        k.undo()
        check("trash paths undo restores it", tree(src) == before)

        r, inv, twin = listed(k, src)
        done = k.req({"c": "transfer", "op": "copy", "rows": [inv], "dest": dst, "listing": r["listing"]}, lambda o: o.get("t") in ("transferdone", "error"))
        check("copy rows copies the invalid file", done and done.get("ok") == 1 and tree(dst) == {INVALID: ("f", b"I")}, "%s %r" % (done, tree(dst)))
        k.undo()
        check("copy undo empties the destination", tree(dst) == {}, repr(tree(dst)))

        r, inv, twin = listed(k, src)
        done = k.req({"c": "transfer", "op": "move", "rows": [inv], "dest": dst, "listing": r["listing"]}, lambda o: o.get("t") in ("transferdone", "error"))
        check("move rows moves the invalid file", done and done.get("ok") == 1 and tree(dst) == {INVALID: ("f", b"I")} and tree(src) == {TWIN: ("f", b"VV")}, "%s %r" % (done, tree(dst)))
        k.undo()
        check("move undo puts it back", tree(src) == before and tree(dst) == {}, repr(tree(src)))

        r, inv, twin = listed(k, src)
        path = src + "/" + r["rows"][inv]["n"]
        done = k.req({"c": "transfer", "op": "copy", "paths": [path], "dest": dst}, lambda o: o.get("t") in ("transferdone", "error"))
        check("copy paths copies the invalid file", done and done.get("ok") == 1 and tree(dst) == {INVALID: ("f", b"I")}, "%s %r" % (done, tree(dst)))
        asked = k.req({"c": "collisions", "id": 21, "paths": [path], "dest": dst}, lambda o: o.get("t") == "collisions")
        name = ((asked or {}).get("names") or [{}])[0]
        check("collisions carries the exact name in x", asked and asked.get("total") == 1 and wire_bytes(name.get("x", "")) == INVALID and "\0" not in name.get("n", ""), str(asked))
        k.undo()

        r, inv, twin = listed(k, src)
        got = k.req({"c": "paths", "rows": [inv]}, lambda o: o.get("t") == "paths")
        check("paths answers the exact path", got and wire_bytes(got["paths"][0]) == os.fsencode(src) + b"/" + INVALID, str(got))

        st = k.req({"c": "settags", "rows": [inv], "op": "toggle", "tag": "Red", "listing": r["listing"]}, lambda o: o.get("t") in ("settagged", "error"))
        try:
            mark = os.getxattr(os.path.join(os.fsencode(src), INVALID), "user.xdg.tags")
        except OSError as e:
            mark = e.errno
        try:
            twin_mark = os.getxattr(os.path.join(os.fsencode(src), TWIN), "user.xdg.tags")
        except OSError as e:
            twin_mark = e.errno
        if mark == 95:
            check("settags rows (no user xattrs here, skipped)", True)
        else:
            check("settags rows tags the invalid file", st and st.get("ok") == 1 and mark == b"Red" and twin_mark != b"Red", "%s %r %r" % (st, mark, twin_mark))

        path = src + "/" + r["rows"][inv]["n"]
        rn = k.req({"c": "rename", "path": path, "to": "renamed.txt"}, lambda o: o.get("t") in ("renamed", "error"))
        check("rename renames the invalid file", rn and rn.get("ok") is True and tree(src) == {b"renamed.txt": ("f", b"I"), TWIN: ("f", b"VV")}, "%s %r" % (rn, tree(src)))
        k.undo()
        check("rename undo puts the name back", tree(src) == before, repr(tree(src)))

        r, inv, twin = listed(k, src)
        path = src + "/" + r["rows"][inv]["n"]
        dup = k.req({"c": "duplicate", "path": path}, lambda o: o.get("t") in ("duplicated", "error"))
        made = set(tree(src)) - set(before)
        check("duplicate copies the invalid file", dup and dup.get("ok") is True and len(made) == 1 and tree(src)[made.pop()] == ("f", b"I"), "%s %r" % (dup, tree(src)))
        k.undo()

        sub = os.path.join(os.fsencode(root), b"pair", b"d\xe9j\xe0")
        os.makedirs(sub)
        with open(os.path.join(sub, b"inner.txt"), "wb") as f:
            f.write(b"in")
        up = k.list(os.path.join(root, "pair"))
        d = row_of(up["rows"], b"d\xe9j\xe0")
        inner = k.list(os.path.join(root, "pair") + "/" + up["rows"][d]["n"]) if d is not None else None
        check("list opens a folder whose name is not UTF-8", inner and inner.get("t") == "rows" and [x["n"] for x in inner["rows"]] == ["inner.txt"] and inner["rows"][0]["s"] == 2, str(inner)[:300])
        # flea --open takes the raw bytes the window's launcher decodes from the row text, and a folder answers status 3.
        got = subprocess.run([os.fsencode(BIN), b"--open", sub], env=k.env, cwd=home, stdin=subprocess.DEVNULL, capture_output=True, timeout=30)
        check("open resolves a folder whose name is not UTF-8", got.returncode == 3, "%s %r" % (got.returncode, got.stderr[-200:]))

        r, inv, twin = listed(k, src)
        sn = k.req({"c": "menuaction", "op": "snapshot", "id": 21, "rows": [inv], "listing": r["listing"]}, lambda o: o.get("t") == "menuaction")
        check("menu snapshot inspects the invalid file", sn and sn.get("ok") is True, str(sn))
        pd = k.req({"c": "menuaction", "op": "prepareDelete", "id": 21}, lambda o: o.get("t") == "menuaction")
        check("menu prepareDelete accepts it", pd and pd.get("ok") is True, str(pd))
        dl = None
        if pd and pd.get("token"):
            dl = k.req({"c": "menuaction", "op": "delete", "id": 21, "token": pd["token"]}, lambda o: o.get("t") == "menuaction" and o.get("op") == "delete")
        left = tree(src)
        check("menu delete destroys the invalid file and never its twin", dl and dl.get("deleted") == 1 and INVALID not in left and TWIN in left, "%s %s %r" % (pd, dl, left))

        alone = os.path.join(root, "alone")
        os.makedirs(alone)
        with open(os.path.join(os.fsencode(alone), b"bad\xff.txt"), "wb") as f:
            f.write(b"AAAA")
        r = k.list(alone)
        check("an invalid name alone lists with its size", r and r["rows"][0]["s"] == 4, str(r))
        t = k.req({"c": "trash", "id": 9, "rows": [0], "listing": r["listing"]}, lambda o: o.get("t") in ("trashed", "error"))
        check("an invalid name alone trashes", t and t.get("ok") == 1 and tree(alone) == {}, "%s %r" % (t, tree(alone)))
        k.undo()
        check("and undo restores it", tree(alone) == {b"bad\xff.txt": ("f", b"AAAA")}, repr(tree(alone)))
        start = {b"bad\xff.txt": ("f", b"AAAA")}

        r = k.list(alone)
        path = alone + "/" + r["rows"][0]["n"]
        rn = k.req({"c": "rename", "path": path, "to": "plain.txt"}, lambda o: o.get("t") in ("renamed", "error"))
        check("an invalid name alone renames", rn and rn.get("ok") is True and tree(alone) == {b"plain.txt": ("f", b"AAAA")}, "%s %r" % (rn, tree(alone)))
        k.undo()
        check("and rename undo restores it", tree(alone) == start, repr(tree(alone)))

        r = k.list(alone)
        dup = k.req({"c": "duplicate", "path": path}, lambda o: o.get("t") in ("duplicated", "error"))
        made = set(tree(alone)) - set(start)
        check("an invalid name alone duplicates", dup and dup.get("ok") is True and len(made) == 1 and tree(alone)[made.pop()] == ("f", b"AAAA"), "%s %r" % (dup, tree(alone)))
        k.undo()
        check("and duplicate undo removes the copy", tree(alone) == start, repr(tree(alone)))

        # The lone file has no twin, so only a stat of its own bytes counts its one line.
        r = k.list(alone)
        meta = k.req({"c": "meta", "row": 0, "text": True, "token": 7}, lambda o: o.get("t") in ("meta", "error"))
        check("meta reads the invalid file itself", meta and meta.get("t") == "meta" and meta.get("lines") == 1 and meta.get("lfailed") is False, str(meta))

        code = k.req({"c": "code", "token": 4, "slot": "look", "path": path}, lambda o: o.get("t") in ("code", "error"))
        check("an invalid name alone previews its own text", code and code.get("t") == "code" and "AAAA" in json.dumps(code.get("lines")), str(code)[:300])

        hits = k.collect({"c": "search", "path": alone, "query": "bad", "hidden": True}, lambda o: o.get("t") in ("searched", "error"))
        window = k.req({"c": "window", "start": 0, "count": 10}, lambda o: o.get("t") in ("rows", "error"))
        rows = (window or {}).get("rows", [])
        check("search finds the invalid name by its own bytes", [wire_bytes(row["n"]) for row in rows] == [b"bad\xff.txt"] and rows[0]["s"] == 4, "%s %s" % (str(hits)[:300], window))
        listing = (window or {}).get("listing")
        got = k.req({"c": "paths", "rows": [0], "listing": listing}, lambda o: o.get("t") in ("paths", "error"))
        check("a search hit resolves to the exact file", got and got.get("t") == "paths" and wire_bytes(got["paths"][0]) == os.fsencode(alone) + b"/bad\xff.txt", str(got))

        hits = k.collect({"c": "searchcontents", "path": alone, "query": "AAAA", "gen": 1, "count": 10}, lambda o: o.get("t") in ("searched", "error"))
        matches = [m for one in hits if one.get("t") == "contentsmatches" for m in one.get("matches", [])]
        check("contents search hits the invalid name and keeps it operable", len(matches) == 1 and bytes.fromhex(matches[0]["pathBytes"]) == b"bad\xff.txt" and matches[0]["operable"] is True and wire_bytes(matches[0]["path"]) == b"bad\xff.txt", str(hits)[:400])
        listing = next((one.get("listing") for one in hits if one.get("t") in ("rows", "contentsmatches") and "listing" in one), None)
        got = k.req({"c": "paths", "rows": [0], "listing": listing}, lambda o: o.get("t") in ("paths", "error"))
        check("a contents hit resolves to the exact file", got and got.get("t") == "paths" and wire_bytes(got["paths"][0]) == os.fsencode(alone) + b"/bad\xff.txt", str(got))
        k.req({"c": "searchclose"}, lambda o: o.get("t") in ("searchclosed", "error"), timeout=5)

        r = k.list(alone)
        sn = k.req({"c": "menuaction", "op": "snapshot", "id": 31, "rows": [0], "listing": r["listing"]}, lambda o: o.get("t") == "menuaction")
        pd = k.req({"c": "menuaction", "op": "prepareDelete", "id": 31}, lambda o: o.get("t") == "menuaction")
        dl = None
        if sn and pd and pd.get("token"):
            dl = k.req({"c": "menuaction", "op": "delete", "id": 31, "token": pd["token"]}, lambda o: o.get("t") == "menuaction" and o.get("op") == "delete")
        check("an invalid name alone deletes permanently", dl and dl.get("deleted") == 1 and tree(alone) == {}, "%s %s %s %r" % (sn, pd, dl, tree(alone)))
        with open(os.path.join(os.fsencode(alone), b"bad\xff.txt"), "wb") as f:
            f.write(b"AAAA")

        # The tail names the real file's bytes but the prefix is not their lossy spelling, so only the strict decoder refuses it.
        forged = os.path.join(alone, "x\0" + b"bad\xff.txt".hex())
        t = k.req({"c": "trash", "id": 10, "paths": [forged]}, lambda o: o.get("t") in ("trashed", "error"))
        check("a forged exact tail is refused", tree(alone) == {b"bad\xff.txt": ("f", b"AAAA")}, "%s %r" % (t, tree(alone)))
    finally:
        k.quit()


def main():
    root = tempfile.mkdtemp(prefix="flea-test-exact-names-")
    open(os.path.join(root, MARKER), "w").close()
    try:
        home = os.path.join(root, "home")
        os.makedirs(home)
        work = os.path.join(root, "work")
        os.makedirs(work)
        run(work, home)
    finally:
        assert os.path.isabs(root) and os.path.exists(os.path.join(root, MARKER))
        shutil.rmtree(root)
    if FAILS:
        print("FAILED: " + ", ".join(FAILS))
        sys.exit(1)


main()
