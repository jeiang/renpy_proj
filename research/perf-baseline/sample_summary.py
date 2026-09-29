#!/usr/bin/env python3
"""Summarise macOS `sample` output: per-thread busy share and top-of-stack functions.
usage: sample_summary.py out/<label>/<run>/sample.txt [TOP=12]
Busy = samples whose top frame is not a known wait/park primitive. Shares are of the sampled wall time (10 ms interval)."""
import collections, re, sys

WAIT = ("__psynch_cvwait", "__workq_kernreturn", "mach_msg2_trap", "mach_msg_trap", "semaphore_wait_signal_trap", "semaphore_timedwait_trap",
        "__semwait_signal", "__select", "kevent", "_dispatch_workloop_worker_thread", "start_wqthread", "__ulock_wait", "__ulock_wait2",
        "__sigsuspend", "nanosleep", "usleep", "__psynch_mutexwait", "poll", "read", "__read_nocancel", "_pthread_cond_wait", "SDL_CondWait", "SDL_SemWait")
path = sys.argv[1]; top_n = int(sys.argv[2]) if len(sys.argv) > 2 else 12
lines = open(path, errors="replace").read().splitlines()
# call graph: top-level lines "    N Thread_x: name" then indented tree; leaf accounting via "Sort by top of stack" per whole process.
threads = []
total = None
for l in lines:
    m = re.match(r"^\s{4}(\d+) (Thread_\d+)(?::\s*(.*?))?(?:\s{2,}.*)?$", l)
    if m:
        threads.append([int(m.group(1)), (m.group(3) or m.group(2)).strip(), 0])
        total = total or int(m.group(1))
tops = collections.Counter()
in_top = False
for l in lines:
    if l.startswith("Sort by top of stack"):
        in_top = True; continue
    if in_top:
        if not l.strip() or l.startswith("Binary Images"):
            break
        m = re.match(r"^\s+(.*?)\s+\(in ([^)]*)\)\s+(\d+)$", l)
        if m:
            tops[(m.group(1), m.group(2))] += int(m.group(3))
if not tops:
    print("no data"); sys.exit(1)
tot = sum(tops.values())
busy = sum(v for (fn, lib), v in tops.items() if fn not in WAIT)
nsamples = max(t[0] for t in threads) if threads else 1
print(f"samples (all threads): {tot}; sampled duration ~{nsamples} samples; busy thread-samples {busy} = {busy / nsamples:.2f} cores")
print("threads (name: samples):", ", ".join(f"{n}" for _, n, _ in threads[:0]))
print("top of stack, non-wait, share of one core-equivalent (samples/duration):")
for (fn, lib), v in sorted(tops.items(), key=lambda kv: -kv[1]):
    if fn in WAIT:
        continue
    print(f"  {v / nsamples * 100:6.1f}%  {fn}  [{lib}]")
    top_n -= 1
    if top_n <= 0:
        break
