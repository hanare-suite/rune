#!/usr/bin/env python3
"""The F0 numbers: runs the perf harness and times a fresh process to its first pixels.

    cargo build --profile perf -p paged-spike --bin paged
    cargo build --profile perf -p paged-spike --features profile --bin paged-perf
    python3 spikes/paged/perf.py

Writes what it measured to `evidence/perf.txt` and prints it.
"""
import json, os, platform, selectors, statistics, subprocess, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))


def sysctl(name):
    return subprocess.run(["sysctl", "-n", name], capture_output=True, text=True).stdout.strip()


lines = [f"MACHINE cpu={sysctl('machdep.cpu.brand_string')!r} cores={sysctl('hw.ncpu')} "
         f"memory_gb={int(sysctl('hw.memsize') or 0) // 2**30} os={platform.mac_ver()[0]!r}"]
out = subprocess.run([os.path.join(ROOT, "target/perf/paged-perf"), "--gpu"], capture_output=True, text=True, check=True)
lines += out.stdout.strip().splitlines()

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
