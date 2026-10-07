# ADR-0048: The CUDA batch lane is deferred — the gate is unopened and the pinned Docker lanes cannot see the GPU

- **Status:** Accepted — deferred, not measured (Phase 15.8)
- **Date:** 2026-10-07

## Context

The Phase-15 plan makes a GPU batch lane **conditional**: build it "only if CPU
profiling shows decode/scan bandwidth dominates." The relevant profiling is the
15.4 worker sweep and the 15.5 DEFLATE ablation. A design subagent
(`research/subagents/phase-15/design-15.8-cuda.md`) evaluated the conditions
before any implementation.

This is an **explicit, reasoned deferral, not a silent omission.**

## Decision

**Do not build a CUDA lane in Phase 15.** Record the deferral with its reasons;
do not add a GPU crate, image, or service.

## Consequences — the reasons, plainly

1. **The gate is unopened.** The pre-registered gate (G1: inflate CPU ≥ 40 % of
   `field-ingest` wall for the ≥ 10 MiB class; G2: marginal throughput per worker
   < 10 % by 4 workers while `User + System` CPU keeps rising) is not opened by
   the 15.4/15.5 results. 15.5's own outcome — `miniz-simd` is single-digit % and
   the CPU inflate backends are within ~1.6× of each other — already makes G1 a
   high bar. Nothing can be read from the gate because the gate says "no".
2. **Docker on this host cannot see the GPU.** `/etc/docker/daemon.json`
   registers no runtimes, and `docker info` lists only `io.containerd.runc.v2`
   and `runc`; no NVIDIA container toolkit is installed. `gpus:` / `runtime:
   nvidia` cannot be satisfied, so a container cannot receive `/dev/nvidia*`. The
   host *has* a GPU, which is exactly the trap: a hand-rolled `docker run
   --device=…` is not the pinned lane and would fail without the toolkit's driver
   mount.
3. **`nvCOMP` is proprietary** and is not on the `deny.toml` allow-list, while
   `cargo-deny` runs `[graph] all-features = true`. Shipping it would be a
   deliberate policy change; git dependencies are denied.
4. **The repo requires Docker-reproducible evidence** (`AGENTS.md` "Docker only";
   ADR-0003). An unreproducible lane is not admissible, and this host cannot run
   one.

## If ever pursued (recorded, not built)

The only plausible kernel is a **batched DEFLATE inflate** over ZIP `method 8`
members and PDF `FlateDecode` streams — embarrassingly parallel, CPU-inflate
bound. It would plug in **as a derived (`Q_gen`) accelerator only**: the GPU
result must be byte-identical to `derive::inflate_raw_deflate` / `inflate_zlib`,
the exact leaf stays the raw compressed span, and `materialize` is unchanged. Byte
scanning and UTF-8 validation are **not** candidates (`memchr`/`simdutf8` already
run at memory bandwidth; transfer dominates). Crossover must auto-decline below a
pre-registered static floor (e.g. `B < 64 MiB` or `k < 64`). Any `unsafe` FFI
would live in an excluded, `publish = false` bench so the library's
`forbid(unsafe_code)` stays intact.

## References

- `research/subagents/phase-15/design-15.8-cuda.md` (the design record)
- `docker info`, `/etc/docker/daemon.json`, `deny.toml`, `compose.yaml`,
  `Dockerfile` (the facts the deferral rests on)
- `docs/phases/phase-15-results.md` (15.8); `AGENTS.md` (Docker only); ADR-0003
