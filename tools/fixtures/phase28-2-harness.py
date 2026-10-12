#!/usr/bin/env python3
# Phase 28.2 — shared measurement harness for the competitor envelope.
#
# The Phase-28.2 review found the pre-28.2 economic courts used a **fresh Python
# interpreter per question** as the headline ("cold") for the SQLite / DuckDB /
# pyarrow lanes, so the headline ratio was partly *interpreter startup*, not engine
# performance. This module supplies the two measurements the envelope now reports
# **separately** from cold-process:
#
#   steady_state(fn)  — one persistent process with warm connection/state: a few
#                       discarded warm-ups, then `iters` measured samples, summarised
#                       by min / p50 / p95 / p99 (the headline).
#   cold_cache(fns)   — within one (warm-interpreter) process, open a **fresh**
#                       connection/reader for each question and time a single call;
#                       this isolates the connection/first-touch cost from the
#                       interpreter start-up that a fresh process would add.
#
# Wall times are microseconds (`us`), matching ADR-0054. No third-party imports.
#
# A note on "cold-cache" honesty: the pinned container runs unprivileged, so we
# cannot drop the host/OS page cache (`echo 3 > /proc/sys/vm/drop_caches`). Both
# `cold_cache` and `cold_process` therefore observe a **warm OS page cache**; the
# only difference from `cold_process` is that the interpreter is already resident.
# That limitation is recorded rather than hidden.

import time


def now_us():
    return int(time.monotonic() * 1_000_000)


def pct(sorted_vals, p):
    """Linear-interpolation percentile over an already-sorted list (numpy default)."""
    n = len(sorted_vals)
    if n == 0:
        return 0
    if n == 1:
        return sorted_vals[0]
    pos = (p / 100.0) * (n - 1)
    lo = int(pos)
    hi = min(lo + 1, n - 1)
    frac = pos - lo
    return sorted_vals[lo] + (sorted_vals[hi] - sorted_vals[lo]) * frac


def summarize(samples):
    s = sorted(int(x) for x in samples)
    return {
        "n": len(s),
        "min": s[0],
        "max": s[-1],
        "p50": pct(s, 50),
        "p95": pct(s, 95),
        "p99": pct(s, 99),
        "median": pct(s, 50),
        "samples": list(samples),
    }


def steady_state(fn, warmup=3, iters=15):
    """One persistent state; discard `warmup` calls, then time `iters` calls."""
    for _ in range(warmup):
        fn()
    samples = []
    for _ in range(iters):
        t0 = now_us()
        fn()
        t1 = now_us()
        samples.append(t1 - t0)
    return summarize(samples)


def cold_cache(fns, state_fresh):
    """Per question: build fresh state, then time exactly one call.

    `fns[q](state)` answers question q against a state; `state_fresh()` returns a
    brand-new state (fresh connection / reader).
    """
    out = {}
    for q, fn in fns.items():
        st = state_fresh()
        t0 = now_us()
        fn(st)
        t1 = now_us()
        out[q] = t1 - t0
    return out
