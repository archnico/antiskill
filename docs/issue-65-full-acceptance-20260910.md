# Legacy development issue 65 — full-corpus acceptance (2026-09-10)

> Historical semantic-only milestone. Superseded for legacy development issue 65 product acceptance by `docs/issue-65-persistent-v3-acceptance-20260911.md`.


## Milestone

Antiskill completed a bounded-memory semantic pass over the full frozen acceptance corpus A.
This is the first full-corpus validation of the fused/lean semantic architecture.
It is not yet a fully materialized Collection v2 database and must not be described as one.

## Frozen input

- source member: private frozen corpus artifact (public name withheld)
- bytes: `13,062,829,132`
- BLAKE3: `7ec6f08a542a05a8fe24fd9ef2820f933374fbed96eee69e4f4205879f2e14e2`
- games observed: `13,058,194`
- comparable canonical mainlines: `13,037,358`
- direct fast path: `12,377,261`
- fallback path: `680,933`

## Provenance

- validated source snapshot tar BLAKE3: `288b56c81196d76b1b8a90239057fa324fe4885012c5c5a5a88945247d57e699`
- freeze commit: `8c29ee8980c01f00dbfee197a0f7eaa9e7ac2358`
- previous fused checkpoint: `d025cb7239d0fc08deb7146b5c51d243680abc07`
- validation binary BLAKE3: `bc39774171e7e44fdbde3ae7a4a258f936510f12151066050bc355f77f6a58c4`

## Full benchmark

Baseline configuration:
- wall: `618.66 s`
- max RSS: `9,700 KiB`

Tuned configuration:
- wall: `454.85 s` (`7m 34.85s`)
- max RSS: `12,712 KiB`
- throughput: about `28,709 games/s`
- wall-time reduction vs baseline: about `26.48%`

Both runs produced exactly the same:
- direct/fallback/game counts;
- mainline digest `ddbacafc6adf111e9d78f1ffdd97c7aafad5b350c47f34d7abd3c18db304a32b`;
- compact80 BLAKE3 `a86fe0237c2b721e13d4abcaf86ca4e762044ca2439ee4f082b438b6f1f0a617`.

`FULL_EQUIVALENCE=PASS`; both stderr files are empty.

Lossless source import before the semantic pass took `17.93 s` with max RSS `3,956 KiB`.
The demonstrated optimized import + semantic path therefore takes `472.78 s` (`7m 52.78s`) once the PGN is present locally.

## What this proves

The original legacy issue 65 failure mode is resolved: semantic processing is bounded-memory on a real 13 GB corpus.
The run also validates deterministic canonical-mainline identity at full-corpus scale.

The tag/catalog model is already implemented and tested: tag order, duplicate tags, decoded name/value,
decode status, node provenance and byte/token ranges are preserved. Product projections already expose
`Date`, `White`, `Black`, `Result` and `Event`.

However, the fused fast path intentionally does not persist the full Catalog for every game.
For fast-path games it only resolves semantic tags needed by chess interpretation (`Variant`, `SetUp`, `FEN`).
Therefore this acceptance does not claim that all source tags have already been materialized into a searchable database.

## Next milestone

Connect the validated fused semantic engine to persistent Collection v2 production without restoring the
old stage-by-stage storage and CPU amplification. The target product is a content-addressed, queryable,
provenance-preserving full Collection whose duplicate analysis can run bounded-memory over all games.

Historical reference: the old persistent 250k run took `8:39:19` for ingest and produced a workspace
about `10.966x` the source size; extrapolating that design to the full corpus was operationally unacceptable.
The new persistence path must retain the semantic guarantees while inheriting the fused engine's economics.
