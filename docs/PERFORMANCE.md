# Product performance classes and budgets v1

This document defines the performance contract for Antiskill's stable product
CLI and JSON surfaces. It classifies user-visible work; it does not prescribe an
implementation or claim that current commands already meet a budget.

The product boundary is:

```text
Antiskill core → stable CLI / JSON interfaces → future En Croissant and other consumers
```

The classes describe product expectations, not whether a command is read-only
or which algorithm it currently uses. Every comparison is meaningful only for a
defined, reproducible benchmark scenario.

## Classes

### Class A — interactive consultation

Class A covers operations used directly in an interaction, where an ordinary
request on a realistic local archive should feel immediate. The initial
engineering budgets are:

- ideal response: `< 150 ms`;
- normal response: `< 500 ms`;
- common warm queries should normally remain `< 1 s`.

These are benchmark budgets for defined scenarios, not hardware-independent
latency guarantees. A scenario must bound the archive, query, result count,
output, and relevant I/O conditions. The budgets are targets to measure against;
they are not evidence that an unmeasured command currently complies.

Class A currently includes:

- `antiskill search` with human-readable output: interactive archive
  consultation. Result count and rendered bytes are part of the scenario.
- `antiskill search --format json`: the same interactive expectation for the
  versioned `antiskill.search.v1` subprocess interface. Serialization and output
  bytes are part of the measured response.
- `antiskill export GAME_ID OUTPUT`: interactive retrieval of one selected
  occurrence. Reading and writing the exact occurrence is size-dependent, so
  occurrence bytes and output storage conditions must be recorded.

These assignments are consumer-facing requirements; they do not assert that
current search or occurrence-resolution work is independent of archive size.
Class A latency includes process startup and ends when the complete requested
output has been emitted or the command has completed its normal
publication/write path, as applicable. A command does not become Class A
merely because it is read-only.

### Class B — heavy analysis

Class B covers analysis whose value can justify seconds, minutes, or longer:
for example future engine analysis, scientific analysis, or substantial
reports. No current stable product command is assigned to Class B.

When Class B capabilities are introduced, long work should expose useful
progress, support interruption and resume where reasonable, reuse valid prior
results where semantics allow it, distinguish complete, partial, and in-progress
results, and avoid unnecessarily blocking Class A consultation. This is a
requirement for future capabilities, not a claim that such infrastructure,
caches, or background execution exist today.

### Class C — corpus-, input-, or batch-scaled work

Class C covers operations whose cost materially follows corpus size, input
bytes, verification work, or requested batch output. They have no Class A
latency budget; benchmarks should emphasize throughput, total time, and relevant
resource use.

Current Class C commands are:

- `antiskill ingest DIR`: parses and builds an active snapshot from input-scale
  PGN data.
- `antiskill compare DIR`: verifies the active archive and scans and hashes the
  supplied snapshot, so work follows both archive and input.
- `antiskill doctor`: verifies the active persisted chain. Although read-only,
  its work scales with the archive and verification chain.
- `antiskill duplicates`: performs corpus-wide metadata, exact-byte, and
  canonical-mainline duplicate analysis.
- `antiskill export --batch IDS_FILE OUTPUT_DIR`: scales with selected
  occurrence count and bytes written. It is batch-scaled even though the request
  is a selected set rather than an arbitrary corpus scan.

Class C design should use bounded-memory or streaming processing where
appropriate, preserve deterministic or incremental work, avoid recomputing
unchanged data where semantics permit, and avoid accidental quadratic behavior
on ordinary corpus sizes without explicit justification. Checkpoint and resume
are future capabilities to add only when genuinely long jobs justify them.
Resource-bounded ingest and interactive latency are separate concerns: improving
one does not reclassify or prove the other.

`antiskill init DIR` is a small, one-time setup operation outside the recurring
A/B/C workloads. It has no v1 latency budget.

## Reproducible benchmark record

Every published or review-relevant product benchmark must record enough context
for another contributor to reproduce or compare it:

- Antiskill version and exact commit, build profile, and relevant feature or
  build options;
- exact command, options, query values, and what marks completion;
- dataset identity or fingerprint, provenance description, source count, game
  or occurrence count, and total relevant bytes;
- result count and output bytes; for batch work, requested occurrence count and
  exported bytes where meaningful;
- warm or cold conditions, defined explicitly, including warm-up steps and
  relevant operating-system/filesystem state;
- hardware, storage type, operating system, filesystem, and other material
  execution conditions;
- wall-clock latency or throughput, with the measurement tool and interval;
- peak or otherwise relevant CPU, memory, and I/O measurements when meaningful,
  including the measurement tool;
- number of repetitions, treatment of warm-ups and outliers, per-run values or
  their location, and the reported summary statistic.

The command must succeed and the benchmark record must identify or fingerprint
the relevant output so that faster incorrect work is not accepted. Synthetic or
rights-compatible user-supplied local corpora are suitable; restricted corpus
data need not and must not be committed merely to make its fingerprint public.

## Class A regressions

A measured Class A regression requires comparable benchmark records and
repeated runs; one isolated timing is not sufficient. It exists when the chosen
summary statistic shows a reproducible deterioration beyond the baseline's
recorded variability and either:

- crosses an applicable 150 ms, 500 ms, or common-warm 1 s boundary; or
- exceeds a stricter scenario-specific regression threshold declared before the
  comparison.

Crossing from below to at or above a boundary counts as crossing it. A first
measurement that already misses a budget identifies an existing performance
gap, not a regression against an unmeasured baseline.

A regression should be treated as a product-contract failure: investigate the
cause, keep the evidence with the change, and fix it before release or record an
explicitly reviewed exception with scope and rationale. Budgets must not be
quietly relaxed to make a result pass. Correctness, lossless bytes, identity,
provenance, and fail-closed verification remain non-negotiable; performance work
must not bypass them.

## Current contract and future consumers

Today this contract applies to the stable local CLI and its documented JSON
surface. Future GUI or subprocess consumers should depend on those interfaces
and inherit the same class expectations. This document does not define or imply
a REST service, server, GUI integration, cache, index, benchmark harness, or new
product command.

## Measured baselines

- [Class A performance baseline v1](PERFORMANCE_BASELINE_V1.md) records the
  first repeated warm measurements of the current Class A surfaces.
