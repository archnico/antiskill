# Class A performance baseline v1

This document records the first reproducible performance baseline for
Antiskill's current Class A product surfaces. It measures the implementation as
it existed before any performance optimization prompted by these results.

The budgets and classifications used here are defined in
[`PERFORMANCE.md`](PERFORMANCE.md).

## Result

All measured Class A scenarios show an **existing performance gap** on this
benchmark.

Warm median latency is approximately 31–32 seconds for every measured surface,
including zero-result search, one-result search, 49-result search, JSON output,
and single-game export. All therefore miss the current common-warm `< 1 s`
budget by more than 30x on this environment.

Because this is the first measured baseline, these misses are not classified as
regressions.

No optimization, cache, index, schema change, parser change, provenance change,
or verification relaxation is part of this baseline work.

## Antiskill build

- version: `antiskill 0.2.0`
- commit:
  `a479ed4878fc3add995b6a76e86e658bcc2fb26f`
- profile: Cargo `release`
- build command: `cargo build --release --locked`
- Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`
- Cargo: `cargo 1.97.1 (c980f4866 2026-06-30)`

## Benchmark archive

The benchmark used a local user-supplied single-source Lichess account-game
PGN export. The account identity, local path, and corpus contents are not
committed.

Source corpus:

- PGN sources: 1
- games: 8,303
- source bytes: 7,533,711
- source BLAKE3:
  `9e7cece2b01e91465a563df21207c57b90c35222ac5c7388c406f207fa227c86`

The benchmark copy was verified byte-identical to the original before ingest.

Active workspace after ingest:

- sources: 1
- games: 8,303
- workspace disk bytes: 78,660,555
- workspace regular files: 11
- active Collection logical bytes: 1,214
- active Collection BLAKE3:
  `ba2c24d23a07b394f40d18fe6fc7797b6240b67a7bf91a55dbef3cdf82e8e8e4`

`antiskill doctor` completed successfully before scenario selection and
measurement.

## Environment

- OS: Arch Linux
- kernel: `Linux 7.1.8-arch1-3 x86_64`
- CPU: Intel Core i5-1135G7 @ 2.40 GHz
- CPU topology: 4 cores / 8 threads
- RAM: 23 GiB
- swap: 4 GiB, unused when the environment snapshot was recorded
- storage: Samsung MZVLQ512HBLU-00B00 NVMe SSD
- filesystem: Btrfs
- benchmark workspace and output were on the same local Btrfs/NVMe storage

These results describe this environment and are not hardware-independent
latency guarantees.

## Scenarios

Scenarios were selected and frozen before timing results were observed.

### A0 — zero-result player search

Human:

    antiskill search --player __antiskill_no_match_v1__

JSON:

    antiskill search --player __antiskill_no_match_v1__ --format json

Expected:

- games: 0
- human bytes: 19
- JSON bytes: 44
- human BLAKE3:
  `67096e2a83f0196be412513f3b377b1c7c90887c6c908ba02a3f159a527c29e5`
- JSON BLAKE3:
  `2b0ff2e23ac6e39c4ecf70f43f42f9383800e238052350df3897d48c39540daf`

### A1 — selective player search

Human:

    antiskill search --player a_077

JSON:

    antiskill search --player a_077 --format json

Expected:

- games: 1
- human bytes: 168
- JSON bytes: 227
- human BLAKE3:
  `4684877d576413390905f42f19436286bcb1c41360c830f9172b9b4629e1fba4`
- JSON BLAKE3:
  `d05688e8e803b818d817fbe84ca09296f8bdd7d28352140d77b5314f2ce35e37`

### A2 — broader date search

Human:

    antiskill search --date 2024-04-08

JSON:

    antiskill search --date 2024-04-08 --format json

Expected:

- games: 49
- human bytes: 5,918
- JSON bytes: 9,288
- human BLAKE3:
  `4e9cfab80be69f38fe571e3fa7c5e660804f7d4fc20f556822cdf6b161ed0b9b`
- JSON BLAKE3:
  `b6e820e309aba3e491f95aaea6656af922e5fdff85ea09d76f5d8de3805178e1`

### E1 — single exact export

    antiskill export g1-c542776db9aa60695e2c9176dbeefcb1aa9684bb714fa93d58bdfb1afeed80b0 OUTPUT

Expected:

- selected occurrences: 1
- output bytes: 954
- output BLAKE3:
  `7d17f32cb9eb50b9804347073cc061e7169fb5a2d7f5c646ecee41aa0feb701f`

The destination file was removed before each timed invocation. Removal was
outside the measured interval. The normal Antiskill publication/write path was
inside the measured interval.

## Measurement protocol

Only a warm baseline is claimed.

The benchmark did **not** attempt to call an uncontrolled first invocation
"cold". OS page-cache dropping or another precise and repeatable cold-cache
procedure was not used, so no cold latency result is reported.

For each of the seven scenarios:

1. run 3 warm-up invocations;
2. validate every warm-up output;
3. run 30 measured invocations;
4. validate every measured output byte-for-byte against the frozen expected
   output;
5. rotate scenario order deterministically across measured rounds.

Timing used Python `time.perf_counter_ns()` around `subprocess.run()`.

For search, complete stdout was captured through a subprocess pipe. The measured
interval therefore includes Antiskill process startup and complete output
production, but not terminal rendering.

For export, the interval includes process startup and Antiskill's normal
publication/write path.

No measured invocation failed output validation. Faster but incorrect output
would have aborted the run.

No measured run was discarded as an outlier. All 210 measured values are
included in the summaries and stored in
[`performance-baseline-v1/raw.tsv`](performance-baseline-v1/raw.tsv).
P95 uses the nearest-rank definition over the 30 measured values for each
scenario.

## Warm results

All values are milliseconds.

| Scenario | Results / selected | Output bytes | Min | Median | Mean | P95 | Max |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| A0 human | 0 | 19 | 30,250.83 | 31,405.45 | 32,594.58 | 36,589.45 | 41,522.97 |
| A0 JSON | 0 | 44 | 30,039.84 | 31,441.82 | 32,919.06 | 38,172.95 | 39,443.28 |
| A1 human | 1 | 168 | 29,864.41 | 31,017.48 | 32,406.49 | 38,117.51 | 39,282.36 |
| A1 JSON | 1 | 227 | 29,931.04 | 31,174.93 | 32,692.92 | 39,462.99 | 42,076.60 |
| A2 human | 49 | 5,918 | 30,232.13 | 31,566.89 | 32,331.28 | 37,056.49 | 39,884.32 |
| A2 JSON | 49 | 9,288 | 30,161.22 | 31,495.86 | 32,798.25 | 39,723.64 | 40,648.91 |
| E1 export | 1 | 954 | 30,055.47 | 31,494.12 | 32,568.84 | 38,526.23 | 39,498.19 |

The similarity across result counts and output sizes shows that, in these
scenarios, output size is not the dominant source of the observed latency. This
baseline does not by itself identify the implementation cause.

## Resource observations

Two additional successful warm invocations were profiled with
`/usr/bin/time -v`.

A1 human search:

- elapsed: 28.72 s
- user CPU: 27.00 s
- system CPU: 1.59 s
- CPU utilization: 99%
- maximum resident set size: 5,613,840 KiB
- filesystem inputs reported by `time`: 0
- filesystem outputs reported by `time`: 8
- output BLAKE3 matched the frozen A1-human fingerprint

E1 export:

- elapsed: 29.78 s
- user CPU: 28.04 s
- system CPU: 1.63 s
- CPU utilization: 99%
- maximum resident set size: 5,613,260 KiB
- filesystem inputs reported by `time`: 0
- filesystem outputs reported by `time`: 176
- exported 954 bytes with the frozen export BLAKE3

These observations are consistent with a CPU- and memory-heavy warm execution
path on this archive. They do not establish which internal stage is responsible.

## Budget comparison

The Class A v1 engineering budgets are:

- ideal: `< 150 ms`
- normal: `< 500 ms`
- common warm: normally `< 1 s`

Every measured scenario misses all three boundaries. Median warm latency ranges
from 31,017.48 ms to 31,566.89 ms, so even the least-latent median is more than
31x the common-warm 1-second boundary.

Classification for all seven scenarios:

**existing performance gap**

This baseline must not be used to relax the budgets. A follow-up performance
issue should investigate the dominant work and improve Class A consultation
while preserving correctness, lossless bytes, identity, provenance, and
fail-closed verification.
