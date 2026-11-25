#!/usr/bin/env python3
"""The F0 interaction checks for page flow in a text field, each driven by a headless script.

    cargo build --profile perf -p paged-spike --bin paged
    python3 spikes/paged/check.py [--bin target/perf/paged]

Every check writes the script it ran to `scripts/`, a screenshot and the facts it read from the
element tree to `out/` (ignored by git), and prints PASS or FAIL. Exits 1 when a check fails.
"""
import json, os, struct, subprocess, sys, tempfile, zlib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
BIN = sys.argv[sys.argv.index("--bin") + 1] if "--bin" in sys.argv else os.path.join(ROOT, "target/perf/paged")
SCRIPTS, EVIDENCE = os.path.join(HERE, "scripts"), os.path.join(HERE, "out")
MARGIN, CONTENT_H, STRIDE, LINE = 71.0, 700.0, 858.0, 14.0
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
        out = subprocess.run(cmd, capture_output=True, text=True, env={**os.environ, **(env or {})})
        if out.returncode != 0:
            sys.exit(f"{' '.join(cmd)}\n{out.stdout}{out.stderr}")
        return json.load(open(tree))


def element(tree, key):
    return next(e for e in tree["elements"] if e.get("key") == key)


def caret(tree):
    """The caret in the field's own points: x, y, the page it is on (from 1) and its y on that page."""
    e = element(tree, "editor")
    c = e["caret"]
    x, y = c[0] - e["rect"][0], c[1] - e["rect"][1]
    return {"x": round(x, 2), "y": round(y, 2), "page": int(y // STRIDE) + 1, "y_on_page": round(y % STRIDE, 2),
            "window": c, "byte": e["selection"][1]}


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


def check(name, ok, facts):
    json.dump({"check": name, "pass": bool(ok), **facts}, open(os.path.join(EVIDENCE, name + ".json"), "w"), indent=1, ensure_ascii=False)
    print(("PASS" if ok else "FAIL"), name, json.dumps(facts, ensure_ascii=False))
    if not ok:
        failed.append(name)


def mark(t, name):
    return {"t": t, "type": "mark", "name": name}


def key(t, name, **mods):
    return {"t": t, "type": "key", "name": name, **mods}


def arrows():
    events = [{"t": 0.2, "type": "tap", "x": 300, "y": 785}, mark(0.6, "last-line"),
              key(0.7, "Down"), mark(1.4, "down"), key(1.5, "Up"), mark(2.2, "up")]
    a = caret(run("arrows", events, "last-line"))
    b = caret(run("arrows", events, "down", snap="arrows-down.png"))
    c = caret(run("arrows", events, "up", snap="arrows-up.png"))
    ok = (a["page"] == 1 and a["y_on_page"] == CONTENT_H - LINE and b["page"] == 2 and b["y_on_page"] == 0
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
    # From page 1's second-to-last line into page 2's third. The pages' text meets the gap at 295 and 453.
    events = [wheel, {"t": 0.8, "type": "drag", "x": 300, "y": 270, "dx": 120, "dy": 220, "dur": 0.6}, mark(2.0, "dragged")]
    tree = run("drag", events, "dragged", snap="drag.png")
    e = element(tree, "editor")
    top = e["rect"][1]
    page1_end, page2_start = top + CONTENT_H, top + STRIDE
    rows = selected_rows(os.path.join(EVIDENCE, "drag.png"), e["rect"][0], e["rect"][0] + e["rect"][2])
    on1 = sorted(r for r in rows if r < page1_end)
    in_gap = sorted(r for r in rows if page1_end <= r < page2_start)
    on2 = sorted(r for r in rows if r >= page2_start)
    a, b = e["selection"]
    ok = bool(on1) and bool(on2) and not in_gap and a != b and max(on1) == page1_end - 1 and min(on2) == page2_start
    check("drag", ok, {"selection_bytes": [a, b], "page_1_text_ends_at_y": page1_end, "page_2_text_starts_at_y": page2_start,
                       "selected_rows_on_page_1": [on1[0], on1[-1]] if on1 else None, "selected_rows_in_gap": len(in_gap),
                       "selected_rows_on_page_2": [on2[0], on2[-1]] if on2 else None})


def typing():
    typed = " Hafen Brücke Morgen"
    # Return at the end of page 1's last line ends a paragraph there; Left puts the caret back at its end.
    events = [{"t": 0.2, "type": "tap", "x": 300, "y": 785}, key(0.5, "Right", cmd=True), key(0.7, "Enter"), key(0.9, "Left"),
              mark(1.3, "before"), {"t": 1.5, "type": "type", "text": typed}, mark(2.6, "typed")]
    events += [key(2.8 + 0.05 * i, "Backspace") for i in range(len(typed))]
    events.append(mark(2.8 + 0.05 * len(typed) + 0.8, "erased"))
    a = caret(run("typing", events, "before", snap="typing-before.png"))
    b = caret(run("typing", events, "typed", snap="typing-typed.png"))
    c = caret(run("typing", events, "erased", snap="typing-erased.png"))
    ok = (a["page"] == 1 and a["y_on_page"] == CONTENT_H - LINE and b["page"] == 2 and b["y_on_page"] == 0
          and (c["page"], c["x"], c["y"], c["byte"]) == (a["page"], a["x"], a["y"], a["byte"]))
    check("typing", ok, {"typed": typed, "paragraph_end_before": a, "paragraph_end_after_typing": b, "paragraph_end_after_backspace": c})


def forced_break():
    # The caret opens before "Kapitel 2", whose style breaks the page; Right then Left bring it into view.
    env = {"PAGED_CARET": "Kapitel 2:"}
    events = [key(0.2, "Right"), key(0.4, "Left"), mark(1.2, "heading"), key(1.4, "Left"), mark(2.2, "before")]
    h = caret(run("forced-break", events, "heading", snap="forced-break.png", env=env))
    p = caret(run("forced-break", events, "before", env=env))
    ok = h["y_on_page"] <= 6 and p["page"] == h["page"] - 1 and p["y_on_page"] < CONTENT_H - 3 * LINE and p["byte"] == h["byte"] - 1
    check("forced-break", ok, {"heading_caret": h, "end_of_paragraph_before": p, "room_left_on_its_page": round(CONTENT_H - p["y_on_page"] - LINE, 2)})


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


os.makedirs(SCRIPTS, exist_ok=True)
os.makedirs(EVIDENCE, exist_ok=True)
for c in (pages, arrows, tap, drag, typing, forced_break, fractional_scale, ime):
    c()
sys.exit(f"FAILED: {', '.join(failed)}" if failed else 0)
