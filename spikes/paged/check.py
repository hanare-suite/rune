#!/usr/bin/env python3
"""The interaction checks for a paged, word-processor text field, each driven by a headless script.

    cargo build --profile perf -p paged-spike --bin paged
    cargo build --profile perf -p paged-spike --features profile --bin paged-perf
    python3 spikes/paged/check.py [--bin target/perf/paged] [--only name,name]

Every check writes the script it ran to `scripts/`, a screenshot and the facts it read from the
element tree to `out/` (ignored by git), and prints PASS or FAIL. Exits 1 when a check fails.
"""
import json, os, re, struct, subprocess, sys, tempfile, zlib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
BIN = sys.argv[sys.argv.index("--bin") + 1] if "--bin" in sys.argv else os.path.join(ROOT, "target/perf/paged")
PERF = os.path.join(os.path.dirname(BIN), "paged-perf")
ONLY = sys.argv[sys.argv.index("--only") + 1].split(",") if "--only" in sys.argv else None
SCRIPTS, EVIDENCE = os.path.join(HERE, "scripts"), os.path.join(HERE, "out")
MARGIN, CONTENT_W, CONTENT_H, STRIDE, LINE = 71.0, 453.0, 700.0, 858.0, 14.0
ATOM, ATOM_H = "Tabelle – in dieser Version nicht bearbeitbar", 120.0
failed = []


def run(name, events, at, snap=None, env=None, scale=None):
    """The element tree at mark `at` of `events`, and a screenshot there when `snap` names one."""
    script = os.path.join(SCRIPTS, name + ".json")
    with open(script, "w") as f:
        json.dump({"events": events}, f, indent=1, ensure_ascii=False)
        f.write("\n")
    with tempfile.TemporaryDirectory() as tmp:
        tree = os.path.join(tmp, "tree.json")
        png = os.path.join(EVIDENCE, snap) if snap else os.path.join(tmp, "snap.png")
        cmd = [BIN, "--script", script, "--tree", tree, "--snap", png, "--at", "mark:" + at] + (["--scale", str(scale)] if scale else [])
        out = subprocess.run(cmd, capture_output=True, text=True, env={**os.environ, "PAGED_FACTS": "1", **(env or {})})
        if out.returncode != 0:
            sys.exit(f"{' '.join(cmd)}\n{out.stdout}{out.stderr}")
        return json.load(open(tree))


def marks(name, events, names, env=None, snaps=None):
    """The tree at each of the marks `names`, with a screenshot for those in `snaps`."""
    return {n: run(name, events, n, snap=(snaps or {}).get(n), env=env) for n in names}


def element(tree, key):
    return next(e for e in tree["elements"] if e.get("key") == key)


def caret(tree):
    """The caret in the field's own points: x, y, the page it is on (from 1) and its y on that page."""
    e = element(tree, "editor")
    c = e["caret"]
    x, y = c[0] - e["rect"][0], c[1] - e["rect"][1]
    return {"x": round(x, 2), "y": round(y, 2), "h": c[3], "page": int(y // STRIDE) + 1, "y_on_page": round(y % STRIDE, 2),
            "window": c, "byte": e["selection"][1]}


def facts(tree):
    """What the app says of the paragraph at the caret and of the document (its `facts` line)."""
    text = element(tree, "facts")["text"]
    out = {}
    for k, v in re.findall(r"(\w+)=(\[[^\]]*\]|\S*)", text):
        out[k] = int(v) if re.fullmatch(r"-?\d+", v) else v
    return out


def pixels(path):
    """An 8-bit RGBA PNG's size and rows."""
    data = open(path, "rb").read()
    at, chunks, idat = 8, {}, b""
    while at < len(data):
        n, kind = struct.unpack(">I4s", data[at:at + 8])
        if kind == b"IDAT":
            idat += data[at + 8:at + 8 + n]
        else:
            chunks[kind] = data[at + 8:at + 8 + n]
        at += 12 + n
    w, h, depth, colour = struct.unpack(">IIBB", chunks[b"IHDR"][:10])
    assert (depth, colour) == (8, 6), "an 8-bit RGBA PNG"
    raw, rows, prev, bpp = zlib.decompress(idat), [], bytearray(w * 4), 4
    for y in range(h):
        f, line = raw[y * (w * 4 + 1)], bytearray(raw[y * (w * 4 + 1) + 1:(y + 1) * (w * 4 + 1)])
        for i in range(len(line)):
            a, b, c = (line[i - bpp] if i >= bpp else 0), prev[i], (prev[i - bpp] if i >= bpp else 0)
            if f == 1: line[i] = (line[i] + a) & 255
            elif f == 2: line[i] = (line[i] + b) & 255
            elif f == 3: line[i] = (line[i] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(line)
        prev = line
    return w, h, rows


def selected_rows(path, x0, x1, scale=2):
    """The rows, in points, that hold selection tint between x0 and x1 points: blue well over red."""
    w, h, rows = pixels(path)
    out = set()
    for y in range(h):
        row = rows[y]
        if any(row[x * 4 + 2] - row[x * 4] > 40 and row[x * 4 + 2] > 200 for x in range(int(x0 * scale), int(x1 * scale))):
            out.add(y // scale)
    return out


def box(path, test, x0, x1, y0, y1, scale=2):
    """The bounds, in device px, of the pixels passing `test(r, g, b)` inside a rect given in points."""
    w, h, rows = pixels(path)
    xs, ys = [], []
    for y in range(max(0, int(y0 * scale)), min(h, int(y1 * scale))):
        row = rows[y]
        for x in range(max(0, int(x0 * scale)), min(w, int(x1 * scale))):
            if test(row[x * 4], row[x * 4 + 1], row[x * 4 + 2]):
                xs.append(x)
                ys.append(y)
    return (min(xs), min(ys), max(xs) + 1, max(ys) + 1) if xs else None


def yellow(r, g, b):
    return r > 225 and g > 190 and b < 150


def ink(r, g, b):
    return r < 110 and g < 110 and b < 110


def check(name, ok, found):
    json.dump({"check": name, "pass": bool(ok), **found}, open(os.path.join(EVIDENCE, name + ".json"), "w"), indent=1, ensure_ascii=False)
    print(("PASS" if ok else "FAIL"), name, json.dumps(found, ensure_ascii=False))
    if not ok:
        failed.append(name)


def mark(t, name):
    return {"t": t, "type": "mark", "name": name}


def key(t, name, **mods):
    return {"t": t, "type": "key", "name": name, **mods}


def steps(start, *items):
    """Events 0.25 s apart from `start`: a string is a mark, a tuple a key and its modifiers, a dict an event."""
    out, t = [], start
    for it in items:
        if isinstance(it, str):
            out.append(mark(t, it))
        elif isinstance(it, tuple):
            out.append(key(t, it[0], **{m: True for m in it[1:]}))
        else:
            out.append({"t": t, **it})
        t = round(t + 0.25, 2)
    return out


# ---- F0: page flow -------------------------------------------------------------------------

def arrows():
    events = [{"t": 0.2, "type": "tap", "x": 300, "y": 785}, mark(0.6, "last-line"),
              key(0.7, "Down"), mark(1.4, "down"), key(1.5, "Up"), mark(2.2, "up")]
    a = caret(run("arrows", events, "last-line"))
    b = caret(run("arrows", events, "down", snap="arrows-down.png"))
    c = caret(run("arrows", events, "up", snap="arrows-up.png"))
    ok = (a["page"] == 1 and a["y_on_page"] >= CONTENT_H - 2 * LINE and b["page"] == 2 and b["y_on_page"] == 0
          and abs(b["x"] - a["x"]) < 8 and (c["page"], c["y"], c["byte"]) == (a["page"], a["y"], a["byte"]))
    check("arrows", ok, {"last_line_of_page_1": a, "after_down": b, "after_up": c})


def tap():
    # Scrolled 500 points: page 2's text starts at 95 + 858 - 500 = 453 in the window.
    wheel = {"t": 0.2, "type": "wheel", "x": 380, "y": 500, "dy": -500}
    spot = (341.0, 600.0)
    events = [wheel, {"t": 0.8, "type": "tap", "x": spot[0], "y": spot[1]}, mark(1.4, "tapped")]
    tree = run("tap", events, "tapped", snap="tap.png")
    c = caret(tree)
    x, y, _, h = c["window"]
    ok = c["page"] == 2 and y <= spot[1] <= y + h and abs(x - spot[0]) <= 4
    check("tap", ok, {"tapped_at": spot, "caret": c})


def drag():
    wheel = {"t": 0.2, "type": "wheel", "x": 380, "y": 500, "dy": -500}
    # From page 1's last lines into page 2's third. The pages' text meets the gap at 295 and 453.
    events = [wheel, {"t": 0.8, "type": "drag", "x": 300, "y": 256, "dx": 120, "dy": 234, "dur": 0.6}, mark(2.0, "dragged")]
    tree = run("drag", events, "dragged", snap="drag.png")
    e = element(tree, "editor")
    top = e["rect"][1]
    page1_end, page2_start = top + CONTENT_H, top + STRIDE
    rows = selected_rows(os.path.join(EVIDENCE, "drag.png"), e["rect"][0], e["rect"][0] + e["rect"][2])
    on1 = sorted(r for r in rows if r < page1_end)
    in_gap = sorted(r for r in rows if page1_end <= r < page2_start)
    on2 = sorted(r for r in rows if r >= page2_start)
    a, b = e["selection"]
    ok = bool(on1) and bool(on2) and not in_gap and a != b and min(on2) == page2_start
    check("drag", ok, {"selection_bytes": [a, b], "page_1_text_ends_at_y": page1_end, "page_2_text_starts_at_y": page2_start,
                       "selected_rows_on_page_1": [on1[0], on1[-1]] if on1 else None, "selected_rows_in_gap": len(in_gap),
                       "selected_rows_on_page_2": [on2[0], on2[-1]] if on2 else None})


def typing():
    typed = " Hafen Brücke Morgen"
    # Widow and orphan control off, as F0 ran it: Return at the end of page 1's last line ends a
    # paragraph there, and typing moves its new last line alone onto page 2.
    events = [{"t": 0.2, "type": "tap", "x": 300, "y": 785}, key(0.5, "Right", cmd=True), key(0.7, "Enter"), key(0.9, "Left"),
              mark(1.3, "before"), {"t": 1.5, "type": "type", "text": typed}, mark(2.6, "typed")]
    events += [key(2.8 + 0.05 * i, "Backspace") for i in range(len(typed))]
    events.append(mark(2.8 + 0.05 * len(typed) + 0.8, "erased"))
    off = {"PAGED_KEEP": "0"}
    a = caret(run("typing", events, "before", snap="typing-before.png", env=off))
    b = caret(run("typing", events, "typed", snap="typing-typed.png", env=off))
    c = caret(run("typing", events, "erased", snap="typing-erased.png", env=off))
    ok = (a["page"] == 1 and a["y_on_page"] == CONTENT_H - LINE and b["page"] == 2 and b["y_on_page"] == 0
          and (c["page"], c["x"], c["y"], c["byte"]) == (a["page"], a["x"], a["y"], a["byte"]))
    check("typing", ok, {"typed": typed, "widow_control": "off", "paragraph_end_before": a, "paragraph_end_after_typing": b, "paragraph_end_after_backspace": c})


def forced_break():
    # The caret opens before "Kapitel 2", whose paragraph breaks the page; Right then Left bring it into view.
    env = {"PAGED_CARET": "Kapitel 2:"}
    events = [key(0.2, "Right"), key(0.4, "Left"), mark(1.2, "heading"), key(1.4, "Left"), mark(2.2, "before")]
    t = run("forced-break", events, "heading", snap="forced-break.png", env=env)
    h, f = caret(t), facts(t)
    p = caret(run("forced-break", events, "before", env=env))
    ok = (h["y_on_page"] <= 6 and p["page"] == h["page"] - 1 and p["y_on_page"] < CONTENT_H - 3 * LINE and p["byte"] == h["byte"] - 1
          and f["break"] == "true" and f["block"] == "paragraph")
    check("forced-break", ok, {"heading_caret": h, "heading_paragraph": {k: f[k] for k in ("style", "break", "block")},
                               "end_of_paragraph_before": p, "room_left_on_its_page": round(CONTENT_H - p["y_on_page"] - LINE, 2)})


def fractional_scale():
    # At 1.25, 700 points are 875 px and the 158 points between pages 197.5 px: a layout that rounds the
    # gap adds 0.5 px a page, and by page 45 its text sits 18 points below the sheet's top margin.
    env = {"PAGED_CARET": "Kapitel 6:"}
    events = [key(0.2, "Right"), key(0.4, "Left"), mark(1.2, "heading")]
    h = caret(run("fractional-scale", events, "heading", env=env, scale=1.25))
    page = round(h["y"] / STRIDE)
    off = h["y"] - page * STRIDE
    ok = page >= 40 and abs(off) <= 0.5
    check("fractional-scale", ok, {"scale": 1.25, "heading_caret": h, "page_index": page, "points_off_its_sheets_top_margin": round(off, 2)})


def ime():
    wheel = {"t": 0.2, "type": "wheel", "x": 380, "y": 500, "dy": -500}
    events = [wheel, {"t": 0.8, "type": "tap", "x": 341, "y": 600}, mark(1.2, "caret"),
              {"t": 1.4, "type": "ime", "preedit": "にほんご", "cursor": [12, 12]}, mark(2.0, "composing")]
    before = run("ime", events, "caret")
    tree = run("ime", events, "composing", snap="ime-headless.png")
    c, area = caret(tree), tree.get("imeArea")
    ok = (area is not None and c["page"] == 2 and abs(area[0] - c["window"][0]) < 1 and area[1] == c["window"][1]
          and tree.get("preedit", {}).get("text") == "にほんご" and c["window"][0] > caret(before)["window"][0])
    check("ime-headless", ok, {"caret": c, "ime_area": area, "preedit": tree.get("preedit"), "caret_before_composing": caret(before)["window"]})


def pages():
    tree = run("open", [mark(0.4, "open")], "open", snap="open.png")
    e, status = element(tree, "editor"), element(tree, "status")["text"]
    n = round((e["rect"][3] + 2 * MARGIN + 16) / STRIDE)
    sheets = sum(1 for x in tree["elements"] if (x.get("key") or "").startswith("page"))
    words = len(e["text"].split())
    ok = status.startswith(f"{n} Seiten") and sheets == n and 45 <= n <= 55 and 24000 <= words <= 26000
    check("open", ok, {"status": status, "field_height": e["rect"][3], "pages": n, "sheets_drawn": sheets, "words": words})


# ---- F2a: run marks ------------------------------------------------------------------------

def word_widths(name, needle, snap=None):
    """The widths of the four letters at `needle` and of the four after them."""
    env = {"PAGED_DOC": "features", "PAGED_CARET": needle}
    events = steps(0.2, ("Right",), ("Left",), "start", *[("Right",)] * 4, "mid", *[("Right",)] * 4, "end")
    t = marks(name, events, ["start", "mid", "end"], env=env, snaps={"end": snap} if snap else None)
    a, b, c = (caret(t[k]) for k in ("start", "mid", "end"))
    return round(b["x"] - a["x"], 2), round(c["x"] - b["x"], 2)


def marks_family():
    base = word_widths("marks-family-base", "MMMMiiii in der Grundschrift")
    other = word_widths("marks-family", "MMMMiiii in einer anderen", snap="marks-family.png")
    unknown = word_widths("marks-family-unknown", "MMMMiiii in einer unbekannten")
    # The registered family is monospaced: four M as wide as four i. The unknown one falls back.
    ok = base[0] > 1.5 * base[1] and abs(other[0] - other[1]) < 0.1 and other != base and unknown == base
    check("marks-family", ok, {"widths_of_MMMM_and_iiii": {"field_family": base, "IBM Plex Mono": other, "unknown_family": unknown}})


def marks_highlight():
    env = {"PAGED_DOC": "features", "PAGED_CARET": "markiertes Wort"}
    n = len("markiertes")
    events = steps(0.2, ("Right",), ("Left",), "start", *[("Right",)] * n, "end")
    t = marks("marks-highlight", events, ["start", "end"], env=env, snaps={"end": "marks-highlight.png"})
    a, b = caret(t["start"]), caret(t["end"])
    png = os.path.join(EVIDENCE, "marks-highlight.png")
    wx, wy = a["window"][0], a["window"][1]
    band = box(png, yellow, wx - 6, b["window"][0] + 6, wy - 4, wy + a["h"] + 4)
    w, h, rows = pixels(png)
    corners = band and all(yellow(*rows[y][x * 4:x * 4 + 3]) for x in (band[0], band[2] - 1) for y in (band[1], band[3] - 1))
    # No padding: the band starts where the word starts and ends where it ends. No rounding: its corners are yellow.
    ok = (band is not None and abs(band[0] - 2 * wx) <= 1.5 and abs(band[2] - 2 * b["window"][0]) <= 1.5 and corners
          and band[1] == round(2 * wy) and band[3] == round(2 * (wy + a["h"])))
    # A run in two sizes on one line: one stripe, as tall as the line.
    env2 = {"PAGED_DOC": "features", "PAGED_CARET": "groß und markiert"}
    events2 = steps(0.2, ("Right",), ("Left",), "start", *[("Right",)] * len("groß"), "big", *[("Right",)] * len(" und markiert"), "end")
    t2 = marks("marks-highlight-sizes", events2, ["start", "big", "end"], env=env2, snaps={"end": "marks-highlight-sizes.png"})
    s, m, e = (caret(t2[k]) for k in ("start", "big", "end"))
    png2 = os.path.join(EVIDENCE, "marks-highlight-sizes.png")
    y0, y1 = s["window"][1] - 12, s["window"][1] + s["h"] + 12
    big = box(png2, yellow, s["window"][0], m["window"][0], y0, y1)
    small = box(png2, yellow, m["window"][0] + 1, e["window"][0], y0, y1)
    ok = ok and big is not None and small is not None and (big[1], big[3]) == (small[1], small[3])
    check("marks-highlight", ok, {"word_x_px": [2 * wx, 2 * b["window"][0]], "line_y_px": [2 * wy, 2 * (wy + a["h"])], "band_px": band,
                                  "corners_are_yellow": bool(corners), "two_sizes_band_rows_px": {"18pt": big and [big[1], big[3]], "10.5pt": small and [small[1], small[3]]}})


def marks_baseline():
    found = {}
    for name, needle in (("normal", "HHHH normal"), ("super", "HHHH hoch"), ("sub", "HHHH tief")):
        env = {"PAGED_DOC": "features", "PAGED_CARET": needle}
        events = steps(0.2, ("Right",), ("Left",), "start", *[("Right",)] * 4, "end")
        snap = "marks-baseline.png" if name == "sub" else f"marks-baseline-{name}.png"
        t = marks("marks-baseline-" + name, events, ["start", "end"], env=env, snaps={"end": snap})
        a, b = caret(t["start"]), caret(t["end"])
        if name == "normal":
            line = a
        # Inside the line's own rows: the lines above and below hold ink too.
        png = os.path.join(EVIDENCE, snap)
        letters = box(png, ink, a["window"][0] + 0.5, b["window"][0] - 0.5, line["window"][1] - 1, line["window"][1] + line["h"] + 3)
        found[name] = {"advance_pt": round(b["x"] - a["x"], 2), "ink_px": letters, "ink_height_px": letters[3] - letters[1], "ink_bottom_px": letters[3]}
        if name != "sub":
            os.remove(png)
    n, up, down = found["normal"], found["super"], found["sub"]
    em3 = 10.5 / 3 * 2
    ratio = lambda k: round(found[k]["advance_pt"] / n["advance_pt"], 3)
    ok = (abs(ratio("super") - 0.58) < 0.02 and abs(ratio("sub") - 0.58) < 0.02
          and abs((n["ink_bottom_px"] - up["ink_bottom_px"]) - em3) <= 1 and abs((down["ink_bottom_px"] - n["ink_bottom_px"]) - em3) <= 1
          and 0.48 <= up["ink_height_px"] / n["ink_height_px"] <= 0.68 and 0.48 <= down["ink_height_px"] / n["ink_height_px"] <= 0.68)
    check("marks-baseline", ok, {"letters": found, "advance_ratio": {"super": ratio("super"), "sub": ratio("sub")},
                                 "ink_height_ratio": {"super": round(up["ink_height_px"] / n["ink_height_px"], 3), "sub": round(down["ink_height_px"] / n["ink_height_px"], 3)},
                                 "expected_shift_px": round(em3, 2),
                                 "shift_px": {"super": n["ink_bottom_px"] - up["ink_bottom_px"], "sub": down["ink_bottom_px"] - n["ink_bottom_px"]}})


# ---- F2a: paragraph attributes -------------------------------------------------------------

def line_ends(name, needle, snap=None, wraps=True):
    """The caret at `needle`'s paragraph start, at its first line's end, at its second line's start, and its facts.

    A line that wraps ends in a space that hangs past it: one step back from the line's edge is the end of its last word."""
    env = {"PAGED_DOC": "features", "PAGED_CARET": needle}
    back = [("Left",)] if wraps else []
    events = steps(0.2, ("Right",), ("Left",), "start", ("Right", "cmd"), *back, "end", ("Left", "cmd"), ("Down",), ("Left", "cmd"), "second")
    t = marks(name, events, ["start", "end", "second"], env=env, snaps={"second": snap} if snap else None)
    return caret(t["start"]), caret(t["end"]), caret(t["second"]), facts(t["start"])


def attrs_align():
    left = line_ends("attrs-left", "Links:")
    centre = line_ends("attrs-centre", "Mitte:", wraps=False)
    right = line_ends("attrs-right", "Rechts:", wraps=False)
    block = line_ends("attrs-justified", "Block:", snap="attrs-align.png")
    # The justified paragraph's last line: one step back from the next paragraph's start.
    env = {"PAGED_DOC": "features", "PAGED_CARET": "Einzug:"}
    last = caret(run("attrs-justified-last", steps(0.2, ("Left",), "last"), "last", env=env))
    ok = (left[0]["x"] == 0 and left[1]["x"] < CONTENT_W - 1
          and abs(centre[0]["x"] - (CONTENT_W - centre[1]["x"])) <= 1 and centre[0]["x"] > 20
          and abs(right[1]["x"] - CONTENT_W) <= 1 and right[0]["x"] > 20
          and block[0]["x"] == 0 and abs(block[1]["x"] - CONTENT_W) <= 0.5 and last["x"] < CONTENT_W - 4
          and (left[3]["align"], centre[3]["align"], right[3]["align"], block[3]["align"]) == ("Left", "Center", "Right", "Justified"))
    check("attrs-align", ok, {"content_width": CONTENT_W, "line_start_and_end_x": {"left": [left[0]["x"], left[1]["x"]], "centre": [centre[0]["x"], centre[1]["x"]],
                              "right": [right[0]["x"], right[1]["x"]], "justified_first_line": [block[0]["x"], block[1]["x"]], "justified_last_line_end": last["x"]}})


def attrs_indent():
    plain = line_ends("attrs-indent", "Einzug:", snap="attrs-indent.png")
    hanging = line_ends("attrs-hanging", "Hängend:")
    styled = line_ends("attrs-list-styled", "Punkt eins", wraps=False)
    nested = line_ends("attrs-list-indented", "Punkt zwei", wraps=False)
    one = line_ends("attrs-list-plain", "Erstens", wraps=False)
    ok = ((plain[0]["x"], plain[2]["x"]) == (54, 36) and (hanging[0]["x"], hanging[2]["x"]) == (18, 36)
          and plain[3]["indent"] == "36/18" and hanging[3]["indent"] == "36/-18"
          # A list item that is also styled: the heading style's line, the bullet's indent.
          and (styled[3]["style"], styled[3]["block"], styled[0]["h"]) == (1, "bullet0", 20) and styled[0]["x"] > 0
          # A list item with a left indent of its own: the indent on top of its level's.
          and nested[3]["block"] == "bullet1" and nested[0]["x"] == 2 * one[0]["x"] + 36 and one[0]["x"] > 0)
    check("attrs-indent", ok, {"first_line_and_second_line_x": {"left 36, first line 18": [plain[0]["x"], plain[2]["x"]], "left 36, first line -18": [hanging[0]["x"], hanging[2]["x"]]},
                               "styled_bullet": {"style": styled[3]["style"], "block": styled[3]["block"], "x": styled[0]["x"], "line_height": styled[0]["h"]},
                               "indented_bullet_level_1_x": nested[0]["x"], "numbered_level_0_x": one[0]["x"]})


def attrs_split_merge():
    env = {"PAGED_DOC": "features", "PAGED_CARET": "Teilen:"}
    events = steps(0.2, *[("Right",)] * 12, "whole", ("Enter",), "second", ("Up",), "first", ("Down",), ("Left", "cmd"), ("Backspace",), "merged")
    t = marks("attrs-split-merge", events, ["whole", "second", "first", "merged"], env=env, snaps={"second": "attrs-split.png"})
    w, s, f, m = (facts(t[k]) for k in ("whole", "second", "first", "merged"))
    same = lambda a, b: all(a[k] == b[k] for k in ("style", "align", "indent"))
    ok = (w["break"] == "true" and same(w, s) and same(w, f)
          and s["id"] == w["next_id"] and s["next_id"] == w["next_id"] + 1 and s["paras"] == w["paras"] + 1
          and f["id"] == w["id"] and f["break"] == "true" and s["break"] == "false"
          and m["id"] == w["id"] and m["paras"] == w["paras"] and same(w, m) and m["break"] == "true"
          and element(t["merged"], "editor")["text"] == element(t["whole"], "editor")["text"])
    pick = lambda d: {k: d[k] for k in ("line", "id", "style", "align", "indent", "break", "paras", "next_id", "last")}
    check("attrs-split-merge", ok, {"before": pick(w), "after_return_second_half": pick(s), "after_return_first_half": pick(f), "after_backspace": pick(m)})


# ---- F2a: page rules -----------------------------------------------------------------------

def rule(name, fill, last, keep, snap=None):
    """Where the last paragraph of a `rules` document starts and ends."""
    env = {"PAGED_DOC": f"rules:{fill}:{last}", "PAGED_CARET": "Ziel:", "PAGED_KEEP": "1" if keep else "0"}
    events = steps(0.2, ("Right",), ("Left",), "start", ("Down", "cmd"), "end")
    t = marks(name, events, ["start", "end"], env=env, snaps={"end": snap} if snap else None)
    return caret(t["start"]), caret(t["end"])


def page_rules():
    place = lambda c: [c["page"], round(c["y_on_page"] / LINE) + 1]
    # The paragraph's line count, on a page of its own.
    s, e = rule("rules-measure", 0, "para", True)
    n = round((e["y"] - s["y"]) / LINE) + 1
    found = {"paragraph_lines": n, "lines_a_page_holds": 50}
    # Orphan: 49 lines before it leave one line of room. Without the rule its first line stays alone.
    off, on = rule("rules-orphan-off", 49, "para", False), rule("rules-orphan", 49, "para", True, snap="rules-orphan.png")
    found["orphan"] = {"without_rule_start_end": [place(off[0]), place(off[1])], "with_rule_start_end": [place(on[0]), place(on[1])]}
    ok = n >= 4 and place(off[0]) == [1, 50] and place(on[0]) == [2, 1] and place(on[1]) == [2, n]
    # Widow: room for all but its last line. Without the rule that line goes over alone; with it, two do.
    fill = 50 - (n - 1)
    off, on = rule("rules-widow-off", fill, "para", False), rule("rules-widow", fill, "para", True, snap="rules-widow.png")
    found["widow"] = {"lines_before": fill, "without_rule_start_end": [place(off[0]), place(off[1])], "with_rule_start_end": [place(on[0]), place(on[1])]}
    ok = ok and place(off[1]) == [2, 1] and place(off[0]) == [1, fill + 1] and place(on[0]) == [1, fill + 1] and place(on[1]) == [2, 2]
    # Room above a paragraph: dropped at the top of a page it flowed onto, kept after a forced break.
    flowed, _ = rule("rules-space-flowed", 50, "head", True, snap="rules-space-flowed.png")
    forced, _ = rule("rules-space-forced", 10, "break", True, snap="rules-space-forced.png")
    inside, _ = rule("rules-space-inside", 10, "head", True)
    found["space_before_10pt"] = {"flowed_onto_page_top_y": [flowed["page"], flowed["y_on_page"]], "after_forced_break_y": [forced["page"], forced["y_on_page"]],
                                  "mid_page_y": [inside["page"], inside["y_on_page"]]}
    ok = ok and (flowed["page"], flowed["y_on_page"]) == (2, 0) and (forced["page"], forced["y_on_page"]) == (2, 10) and (inside["page"], inside["y_on_page"]) == (1, 150)
    check("page-rules", ok, found)


# ---- F2a: locked atoms ---------------------------------------------------------------------

def groups(node, out):
    if node.get("role") == "group" and node.get("name"):
        out.append(node["name"])
    for c in node.get("children", []):
        groups(c, out)
    return out


def atoms():
    env = {"PAGED_DOC": "atoms", "PAGED_CARET": "Danach steht"}
    events = steps(0.2, ("Right",), ("Left",), "after-para", ("Left",), "atom-end", ("Left",), "atom-start", ("Left",), "before-para-end",
                   ("Right",), ("Right", "shift"), "selected", ("Right",), {"type": "type", "text": "x"}, "typed",
                   ("Backspace",), ("Backspace",), "back-at-atom", ("Backspace",), "deleted", ("z", "cmd"), "undone")
    names = ["after-para", "atom-end", "atom-start", "before-para-end", "selected", "typed", "back-at-atom", "deleted", "undone"]
    t = marks("atoms", events, names, env=env, snaps={"atom-end": "atoms.png", "selected": "atoms-selected.png", "deleted": "atoms-deleted.png"})
    c = {k: caret(t[k]) for k in names}
    f = {k: facts(t[k]) for k in names}
    text = {k: element(t[k], "editor")["text"] for k in names}
    label = len(ATOM.encode())
    sel = element(t["selected"], "editor")["selection"]
    before, _ = (caret(run("atoms-before", steps(0.2, ("Right",), ("Left",), "at"), "at", env={"PAGED_DOC": "atoms", "PAGED_CARET": "Davor steht"})), None)
    named = {k: groups(t[k]["accessibility"], []) for k in ("after-para", "deleted")}
    ok = (c["after-para"]["byte"] - c["atom-end"]["byte"] == 1 and c["atom-end"]["byte"] - c["atom-start"]["byte"] == label
          and c["atom-start"]["byte"] - c["before-para-end"]["byte"] == 1 and f["atom-end"]["atom"] == "true"
          and sorted(sel) == [c["atom-start"]["byte"], c["atom-end"]["byte"]]
          # Typed beside it: a paragraph of its own, the label untouched.
          and f["typed"]["paras"] == f["after-para"]["paras"] + 1 and ATOM + "\nx\n" in text["typed"] and f["typed"]["atom"] == "false"
          and text["back-at-atom"] == text["after-para"] and c["back-at-atom"]["byte"] == c["atom-end"]["byte"]
          # Deleting it is one edit, and one step of the app's history.
          and f["deleted"]["paras"] == f["after-para"]["paras"] - 1 and ATOM not in text["deleted"]
          and f["deleted"]["events"] == f["back-at-atom"]["events"] + 1 and f["deleted"]["steps"] == f["back-at-atom"]["steps"] + 1
          and f["deleted"]["last"].startswith("deleted:") and "," not in f["deleted"]["last"]
          and text["undone"] == text["after-para"]
          # As tall as the app made it, with the body style's room after it.
          and c["after-para"]["y"] - before["y"] == LINE + 6 + ATOM_H + 6
          and ATOM in named["after-para"] and ATOM not in named["deleted"] and len(named["after-para"]) == 2)
    check("atoms", ok, {"label_bytes": label, "caret_bytes": {k: c[k]["byte"] for k in ("before-para-end", "atom-start", "atom-end", "after-para")},
                        "selection_after_shift_right": sel, "paragraphs": {k: f[k]["paras"] for k in ("after-para", "typed", "back-at-atom", "deleted", "undone")},
                        "delete_reported": f["deleted"]["last"], "history_steps": [f["back-at-atom"]["steps"], f["deleted"]["steps"]],
                        "y_of_paragraph_before_and_after": [before["y"], c["after-para"]["y"]], "atom_height": ATOM_H,
                        "accessibility_groups": named})


# ---- F2a: atoms at a paragraph's edge ------------------------------------------------------

BILD, DIAGRAMM = "Bild – in dieser Version nicht bearbeitbar", "Diagramm – in dieser Version nicht bearbeitbar"
RICH = "<p><b>X</b>y</p><ul><li>L1</li><li>L2</li></ul>"


def listed(f, name):
    """A bracketed fact as its items."""
    return [v for v in f[name].strip("[]").split(",") if v]


def paragraphs(tree):
    """A short document's paragraphs as the app's facts list them: text, id, atom and block of each."""
    f, text = facts(tree), element(tree, "editor")["text"]
    ids, locked, blocks = listed(f, "ids"), listed(f, "atoms"), listed(f, "blocks")
    lines = text.split("\n")
    assert len(lines) == len(ids) == len(locked) == len(blocks), "one fact for each paragraph"
    return [{"text": t, "id": int(i), "atom": a == "1", "block": b} for t, i, a, b in zip(lines, ids, locked, blocks)]


def sound(tree):
    """No two paragraphs share an id, and the app's document followed the field through the reported changes."""
    ids = [p["id"] for p in paragraphs(tree)]
    return len(set(ids)) == len(ids) and 0 not in ids and facts(tree)["sync"] == "true"


def brief(ps):
    return [f'{p["id"]}{"*" if p["atom"] else ""}{":" + p["block"] if p["block"] != "p" else ""} {p["text"][:12]}' for p in ps]


def paste(text, html):
    return {"type": "paste", "text": text, "html": html}


def c_byte(tree):
    return element(tree, "editor")["selection"][1]


def paste_over_atom():
    """A rich paste over the selected last atom: the text goes where the caret landed."""
    env = {"PAGED_DOC": "edges", "PAGED_CARET": DIAGRAMM}
    events = steps(0.2, ("Right",), ("Left",), ("Right", "shift"), "selected", paste("Xy", "<b>X</b>y"), "pasted")
    t = marks("paste-over-atom", events, ["selected", "pasted"], env=env, snaps={"pasted": "paste-over-atom.png"})
    sel = element(t["selected"], "editor")["selection"]
    text, f = element(t["pasted"], "editor")["text"], facts(t["pasted"])
    ps, at = paragraphs(t["pasted"]), len(text[:text.index("Xy")].encode())
    ok = (sel[1] - sel[0] == len(DIAGRAMM.encode()) and text.endswith("Unten steht ein Absatz.Xy") and DIAGRAMM not in text
          and listed(f, "bold") == [f"{at}..{at + 1}"] and c_byte(t["pasted"]) == at + 2 and sound(t["pasted"])
          and not any(p["atom"] and p["text"] == "" for p in ps))
    check("paste-over-atom", ok, {"selection": sel, "paragraphs_after": brief(ps), "bold": listed(f, "bold"), "x_at_byte": at,
                                  "reported": f["last"], "document_in_sync": f["sync"]})


def paste_at_atom():
    """A rich paste at an atom's end and at its start: marks and blocks sit on the pasted lines."""
    found, ok = {}, True
    for where, keys in (("after", [("Right",)]), ("before", [("Right",), ("Left",)])):
        env = {"PAGED_DOC": "atoms", "PAGED_CARET": ATOM}
        name = f"paste-at-atom-{where}"
        t = marks(name, steps(0.2, *keys, "at", paste("Xy\nL1\nL2", RICH), "pasted"), ["at", "pasted"], env=env,
                  snaps={"pasted": "paste-at-atom.png"} if where == "after" else None)
        text, f, ps = element(t["pasted"], "editor")["text"], facts(t["pasted"]), paragraphs(t["pasted"])
        x = len(text[:text.index("Xy\nL1\nL2")].encode())
        i = next(k for k, p in enumerate(ps) if p["text"] == "Xy")
        table = next(k for k, p in enumerate(ps) if p["text"] == ATOM)
        ok = (ok and [p["text"] for p in ps[i:i + 3]] == ["Xy", "L1", "L2"] and [p["block"] for p in ps[i:i + 3]] == ["p", "b", "b"]
              and all(p["block"] == "p" for k, p in enumerate(ps) if k not in (i + 1, i + 2))
              and listed(f, "bold") == [f"{x}..{x + 1}"] and ps[table]["atom"] and [p["atom"] for p in ps].count(True) == 2
              and table == (i - 1 if where == "after" else i + 3) and c_byte(t["pasted"]) == x + len("Xy\nL1\nL2") and sound(t["pasted"]))
        found[where] = {"paragraphs_after": brief(ps), "bold": listed(f, "bold"), "x_at_byte": x, "reported": f["last"], "document_in_sync": f["sync"]}
    check("paste-at-atom", ok, found)


def atom_ids():
    """Text typed over what stands before an atom: no two paragraphs end up with one id."""
    found, ok = {}, True
    cases = {"empty-paragraph": [("Left",), ("Right", "shift")], "atom-to-atom": [("Right", "shift"), ("Right", "shift")]}
    for case, keys in cases.items():
        env = {"PAGED_DOC": "edges", "PAGED_CARET": ATOM}
        t = marks(f"atom-ids-{case}", steps(0.2, ("Right",), ("Left",), *keys, "selected", {"type": "type", "text": "x"}, "typed"),
                  ["selected", "typed"], env=env)
        before, ps, f = paragraphs(t["selected"]), paragraphs(t["typed"]), facts(t["typed"])
        i = next(k for k, p in enumerate(ps) if p["text"] == "x")
        want = ATOM if case == "empty-paragraph" else BILD
        ok = (ok and sound(t["typed"]) and len(ps) == len(before) and ps[i + 1]["text"] == want and ps[i + 1]["atom"] and not ps[i]["atom"]
              and ps[i + 1]["id"] == next(p["id"] for p in before if p["text"] == want)
              and [p["id"] for p in ps[:i] + ps[i + 2:]] == [p["id"] for p in before[:i] + before[i + 2:]])
        found[case] = {"selection": element(t["selected"], "editor")["selection"], "ids_before": [p["id"] for p in before],
                       "paragraphs_after": brief(ps), "reported": f["last"], "document_in_sync": f["sync"]}
    check("atom-ids", ok, found)


def atom_keys():
    """Backspace after an atom and Delete before one select it, and a second press deletes it;
    an empty paragraph beside an atom goes instead."""
    found, ok = {}, True
    label = len(ATOM.encode())
    for case, find, keys, press in (("backspace", "Danach steht", [], "Backspace"), ("delete", "Davor steht", [("Right", "cmd")], "Delete")):
        env = {"PAGED_DOC": "atoms", "PAGED_CARET": find}
        names = ["at", "selected", "deleted", "undone"]
        t = marks(f"atom-keys-{case}", steps(0.2, ("Right",), ("Left",), *keys, "at", (press,), "selected", (press,), "deleted", ("z", "cmd"), "undone"),
                  names, env=env, snaps={"selected": "atom-keys-selected.png"} if case == "backspace" else None)
        f, text = {k: facts(t[k]) for k in names}, {k: element(t[k], "editor")["text"] for k in names}
        sel = sorted(element(t["selected"], "editor")["selection"])
        start = len(text["at"][:text["at"].index(ATOM)].encode())
        ok = (ok and sel == [start, start + label] and text["selected"] == text["at"] and f["selected"]["events"] == f["at"]["events"]
              and ATOM not in text["deleted"] and f["deleted"]["paras"] == f["at"]["paras"] - 1
              and f["deleted"]["last"] == f"deleted:{start}..{start + label + 1}>0"
              and f["deleted"]["events"] == f["at"]["events"] + 1 and f["deleted"]["steps"] == f["at"]["steps"] + 1
              and sound(t["deleted"]) and text["undone"] == text["at"])
        found[case] = {"caret": c_byte(t["at"]), "selection_after_first_press": sel, "atom": [start, start + label],
                       "reported_by_second_press": f["deleted"]["last"], "history_steps": [f["at"]["steps"], f["selected"]["steps"], f["deleted"]["steps"]]}
    # An empty paragraph beside an atom: the first press removes it and leaves the atom.
    for case, find, press in (("empty-after", "Unten steht", "Backspace"), ("empty-before", ATOM, "Delete")):
        env = {"PAGED_DOC": "edges", "PAGED_CARET": find}
        t = marks(f"atom-keys-{case}", steps(0.2, ("Right",), ("Left",), ("Left",), "at", (press,), "pressed"), ["at", "pressed"], env=env)
        before, ps, sel = paragraphs(t["at"]), paragraphs(t["pressed"]), element(t["pressed"], "editor")["selection"]
        line = facts(t["at"])["line"]
        ok = (ok and before[line]["text"] == "" and [p["id"] for p in ps] == [p["id"] for k, p in enumerate(before) if k != line]
              and [p["atom"] for p in ps].count(True) == 3 and sel[0] == sel[1] and sound(t["pressed"]))
        found[case] = {"empty_paragraph": line, "ids_before": [p["id"] for p in before], "paragraphs_after": brief(ps), "selection": sel,
                       "reported": facts(t["pressed"])["last"]}
    check("atom-keys", ok, found)


# ---- F2a: one history, reported changes ----------------------------------------------------

def history():
    env = {"PAGED_DOC": "features", "PAGED_CARET": "Danach:"}
    events = steps(0.2, ("Right",), ("Left",), "open", {"type": "type", "text": "ab"}, "typed",
                   *[("Right", "shift")] * 7, ("F6",), "highlighted", ("b", "cmd"), "bold", ("F1",), "centred",
                   ("z", "cmd"), ("z", "cmd"), ("z", "cmd"), "three-undone", ("z", "cmd"), ("z", "cmd"), "all-undone",
                   ("z", "cmd", "shift"), ("z", "cmd", "shift"), "redone")
    names = ["open", "typed", "highlighted", "bold", "centred", "three-undone", "all-undone", "redone"]
    t = marks("history", events, names, env=env, snaps={"centred": "history.png"})
    f = {k: facts(t[k]) for k in names}
    text = {k: element(t[k], "editor")["text"] for k in names}
    at = element(t["open"], "editor")["selection"][1]
    ok = (all(f[k]["field_history"] == "0/0" for k in names)
          # Two characters typed: two events, each one change and one step.
          and f["typed"]["last"] == f"typed:{at + 1}..{at + 1}>1" and f["typed"]["steps"] == f["open"]["steps"] + 2
          # Marks alone, and attributes alone: each reports the bytes it touched, and is one step.
          and f["highlighted"]["last"] == f"marks:{at + 2}..{at + 9}>7" and f["highlighted"]["steps"] == f["typed"]["steps"] + 1
          and f["bold"]["last"] == f"marks:{at + 2}..{at + 9}>7" and f["bold"]["steps"] == f["highlighted"]["steps"] + 1
          and f["centred"]["last"].startswith("paragraphs:") and f["centred"]["steps"] == f["bold"]["steps"] + 1 and f["centred"]["align"] == "Center"
          and text["typed"] == text["centred"] != text["open"]
          # The app's history takes Cmd+Z: the field passes it on.
          and text["three-undone"] == text["typed"] and f["three-undone"]["align"] == "Left"
          and text["all-undone"] == text["open"] and text["redone"] == text["typed"])
    check("history", ok, {"caret_byte": at, "reported": {k: f[k]["last"] for k in ("typed", "highlighted", "bold", "centred")},
                          "app_history_steps": {k: f[k]["steps"] for k in names}, "field_history": {k: f[k]["field_history"] for k in names},
                          "text_restored_by_undo": text["all-undone"] == text["open"]})


# ---- F2a: rich text reuse, and the page count in the same frame --------------------------------

def perf_lines(args):
    out = subprocess.run([PERF, *args], capture_output=True, text=True)
    if out.returncode != 0:
        sys.exit(f"{PERF} {' '.join(args)}\n{out.stdout}{out.stderr}")
    return out.stdout.strip().splitlines()


def metric(lines, case, name):
    line = next(l for l in lines if l.startswith(f"METRIC case={case} metric={name} "))
    return {k: float(v) for k, v in re.findall(r"(p50|p95|max)=([\d.]+)", line)}


def reuse():
    found = {}
    for label, args in (("51 pages", ["--words", "25000", "--page", "25"]), ("200 pages", ["--words", "100000", "--page", "100"])):
        lines = perf_lines(["--only", "type", "--samples", "80", *args])
        where = next(l for l in lines if l.startswith("CARET"))
        found[label] = {"caret": where, "paragraphs_keyed_per_keystroke": metric(lines, "type", "keyed"),
                        "layouts_shaped_per_keystroke": metric(lines, "type", "shaped"), "text_ms": metric(lines, "type", "text_ms")}
    ok = all(v["paragraphs_keyed_per_keystroke"]["max"] <= 2 for v in found.values())
    # Formatting alone: centring one paragraph keys that paragraph, not every one after it.
    for label, args in (("51 pages", ["--words", "25000", "--page", "25"]), ("200 pages", ["--words", "100000", "--page", "100"])):
        lines = perf_lines(["--only", "centre", "--samples", "20", *args])
        found[label]["paragraphs_keyed_per_centring"] = keyed = metric(lines, "centre", "keyed")
        ok = ok and keyed["max"] <= 1
    check("reuse", ok, found)


def pages_same_frame():
    line = next(l for l in perf_lines(["--only", "pages"]) if l.startswith("PAGES"))
    v = {k: int(n) for k, n in re.findall(r"(\w+)=(\d+)", line)}
    ok = v["after_field"] == v["before_field"] + 1 and v["same_frame_status"] == v["after_field"] and v["same_frame_sheets"] == v["after_field"]
    check("pages-same-frame", ok, v)


CHECKS = {"open": pages, "arrows": arrows, "tap": tap, "drag": drag, "typing": typing, "forced-break": forced_break, "fractional-scale": fractional_scale, "ime-headless": ime,
          "marks-family": marks_family, "marks-highlight": marks_highlight, "marks-baseline": marks_baseline,
          "attrs-align": attrs_align, "attrs-indent": attrs_indent, "attrs-split-merge": attrs_split_merge,
          "page-rules": page_rules, "atoms": atoms, "paste-over-atom": paste_over_atom, "paste-at-atom": paste_at_atom,
          "atom-ids": atom_ids, "atom-keys": atom_keys, "history": history, "reuse": reuse, "pages-same-frame": pages_same_frame}
os.makedirs(SCRIPTS, exist_ok=True)
os.makedirs(EVIDENCE, exist_ok=True)
for name, c in CHECKS.items():
    if ONLY is None or name in ONLY:
        c()
sys.exit(f"FAILED: {', '.join(failed)}" if failed else 0)
