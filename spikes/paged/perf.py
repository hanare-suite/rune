#!/usr/bin/env python3
"""The numbers: runs the perf harness at 51 pages against F0's limits, again at 200 pages with
no limit, and times a fresh process to its first pixels.

    cargo build --profile perf -p paged-spike --bin paged
    cargo build --profile perf -p paged-spike --features profile --bin paged-perf
    python3 spikes/paged/perf.py

Writes what it measured to `evidence/perf.txt` and prints it. Exits 1 when a 51-page number is
past its limit.
"""
import os, platform, re, selectors, statistics, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
PERF = os.path.join(ROOT, "target/perf/paged-perf")
# F0's limits for the 51-page document: (case, metric, percentile, limit).
LIMITS = [("type", "wall_ms", "p50", 8.0), ("type", "wall_ms", "p95", 16.0), ("scroll", "wall_ms", "p95", 8.3), ("open", "wall_ms", "p50", 500.0)]


def sysctl(name):
    return subprocess.run(["sysctl", "-n", name], capture_output=True, text=True).stdout.strip()


def harness(*args):
    out = subprocess.run([PERF, "--gpu", *args], capture_output=True, text=True)
    if out.returncode != 0:
        sys.exit(f"paged-perf {' '.join(args)}\n{out.stdout}{out.stderr}")
    return out.stdout.strip().splitlines()


def value(lines, case, metric, percentile):
    line = next(l for l in lines if l.startswith(f"METRIC case={case} metric={metric} "))
    return float(re.search(rf"{percentile}=([\d.]+)", line).group(1))


lines = [f"MACHINE cpu={sysctl('machdep.cpu.brand_string')!r} cores={sysctl('hw.ncpu')} "
         f"memory_gb={int(sysctl('hw.memsize') or 0) // 2**30} os={platform.mac_ver()[0]!r}"]
lines.append("RUN pages=51 words=25000 caret_page=25 limits=F0")
small = harness()
lines += small
undo = next(l for l in small if l.startswith("UNDO"))
heap = float(re.search(r"heap_extra_mb=(-?[\d.]+)", undo).group(1))
passed = True
for case, metric, percentile, limit in LIMITS:
    got = value(small, case, metric, percentile)
    passed &= got < limit
    lines.append(f"LIMIT pages=51 case={case} metric={metric} {percentile}={got:.4f} limit={limit} pass={str(got < limit).lower()}")
passed &= heap < 50.0
lines.append(f"LIMIT pages=51 case=undo metric=heap_extra_mb value={heap:.2f} limit=50.0 pass={str(heap < 50.0).lower()}")

# 200 pages: the same cases on a document four times as long, the caret in its middle. Reported, not judged.
lines.append("RUN pages=200 words=100000 caret_page=100 limits=none")
lines += harness("--words", "100000", "--page", "100", "--only", "open,type,enter,scroll")

# A new process each time, from exec to the first frame read back from the GPU; one warm-up run.
runs = []
for i in range(8):
    start = time.perf_counter()
    proc = subprocess.Popen([os.path.join(ROOT, "target/perf/paged"), "--first-pixels", "--frames", "1"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    with selectors.DefaultSelector() as selector:
        selector.register(proc.stdout, selectors.EVENT_READ)
        assert selector.select(timeout=30), "first pixels not ready after 30 s"
        line = proc.stdout.readline().decode()
    elapsed = (time.perf_counter() - start) * 1000
    proc.communicate(timeout=30)
    assert line.startswith("rune-first-pixels "), line
    if i:
        runs.append(elapsed)
runs.sort()
lines.append(f"PROCESS case=open-process metric=wall_ms p50={statistics.median(runs):.1f} max={runs[-1]:.1f} n={len(runs)}")

text = "\n".join(lines) + "\n"
open(os.path.join(HERE, "evidence", "perf.txt"), "w").write(text)
print(text, end="")
sys.exit(0 if passed else "FAILED: a 51-page number is past its limit")
