# Legacy development issue 65 — persistent Collection v3 acceptance (2026-09-11)

## Result

Legacy development issue 65 reached full-corpus product acceptance with the fused persistent Collection v3 path.
The frozen acceptance corpus A was ingested into a content-addressed, lossless workspace,
verified by `antiskill doctor`, and processed by product `antiskill duplicates` with bounded RAM.
The persistent v3 semantic projection is byte-identical to the previously accepted full-corpus `compact80` reference.

No duplicate was deleted. `duplicates` remains diagnostic and non-destructive.

## Frozen input

- source member: private frozen corpus artifact (public name withheld)
- source bytes: `13,062,829,132`
- source BLAKE3: `7ec6f08a542a05a8fe24fd9ef2820f933374fbed96eee69e4f4205879f2e14e2`
- games: `13,058,194`
- direct fast path: `12,377,261`
- fallback path: `680,933`
- comparable canonical mainlines: `13,037,358`
- non-comparable: `20,836`

## Code provenance

- v3 commit: `9c289c9e03a1adc09d5df33cb08a107646f4746b`
- full-ingest binary BLAKE3: `c623257cde7856ce0211ca6b550fa2bff63e5d806fcca5889f92a133ccf7a600`
- scratch-path fix commit: `2d86c6ac41cdbb70ff2541b212062cdf041dd281`
- patched binary BLAKE3: `8488879555e11df2f4eade594661dca10a206ef05ea882007e8b6ab9bd9e1242`
- historical validated commit chain retained at `origin/archive/issue-65-acceptance-20260911`

## Full persistent ingest

Product `antiskill ingest`:

- result: PASS, `13,058,194` games, `0` errors
- wall: `5m19.52s`
- user CPU: `967.76s`
- system CPU: `50.11s`
- maximum RSS: `17,400 KiB`
- SourceIndex bytes: `1,149,121,128`
- SourceIndex BLAKE3: `48490903c4f37c5c98203dfb171e013b3d0c38a9ab3309acb728cc976b4a14fe`
- stable workspace bytes after cleanup: `14,211,951,062` (`du -sb`), about `14.21 GB` decimal / `13.24 GiB`

The SourceIndex is fixed-width: 88 bytes/game plus a 56-byte header.
SourceBlob custody remains the byte-exact authoritative source.

## Full semantic equivalence

Projecting Collection v3 back to the frozen `compact80` representation produced:

`a86fe0237c2b721e13d4abcaf86ca4e762044ca2439ee4f082b438b6f1f0a617`

This exactly matches the previously accepted full semantic probe.
Counts also match exactly: `12,377,261` direct-fast, `680,933` fallback,
`13,037,358` comparable and `20,836` non-comparable.

`FULL_COMPACT80_EQUIVALENCE=PASS`.

## Doctor and idempotent reuse

Full `antiskill doctor` passed repeatedly:

- representative wall: `33.37s`
- maximum RSS: `5,708 KiB`
- post-dedup verification: `28.55s`, `5,644 KiB` RSS

A second full `antiskill ingest` reused the verified SourceIndex:

- wall: `1m48.52s`
- maximum RSS: `5,596 KiB`
- `1 sorgente riusata dal workspace`
- SourceIndex identity unchanged
- Workspace State identity unchanged

`INDEX_REUSE_STABLE=PASS`
`STATE_IDEMPOTENT=PASS`

## Full duplicates

The first full attempt exposed an environment-specific scratch-space limit: `/tmp` on
the acceptance host was a 12 GiB tmpfs. The command failed closed with `Disk quota exceeded`,
published no partial report and did not damage the Collection.

With scratch on the workspace filesystem the full analysis passed:

- wall: `12m45.68s`
- maximum RSS: `65,616 KiB`
- byte-identical duplicate groups: `145`
- same canonical mainline but different PGN groups: `28,266`
- groups with conflicting result metadata: `11,566`
- non-comparable occurrences: `20,836`
- report bytes: `9,626,450`
- report lines: `126,878`
- report BLAKE3: `ddeb77dee4e275f8d8df87c70598a51967846ca10b00e31d663ad6fb6d07eddc`
- observed external-sort scratch peak: about `17G`
- scratch cleanup after success: complete

Commit `2d86c6ac41cdbb70ff2541b212062cdf041dd281` moves duplicate-analysis scratch
to `raw/tmp` inside the workspace, eliminating dependence on the system tmpfs size.

## Scaling evidence

### ~100k-game prefix

- source bytes: `100,673,869`
- segmented games: `99,998` (same as the frozen prior farm runs on this prefix)
- ingest: `2.31s`, max RSS `18,812 KiB`
- SourceIndex: `8,799,880` bytes
- doctor: `0.24s`, RSS `7,388 KiB`
- independent SourceIndex determinism: PASS
- byte-exact export: PASS
- compact80 BLAKE3: `3e8511dab65435949a0c5df678c280bef4389828467a003326f9a6aa6fe29629`
- v3 vs prior accepted compact80: byte-identical

### 1M-game prefix
- source bytes: `1,002,562,989`
- games: `1,000,000`
- ingest: `17.58s`, max RSS `17,572 KiB`
- SourceIndex: `88,000,056` bytes
- doctor: `2.32s`, RSS `5,700 KiB`
- re-ingest reuse: `5.39s`, RSS `5,960 KiB`
- duplicates: `51.09s`, RSS `62,404 KiB`
- compact80 BLAKE3: `b79a1509b023f0f8d333ad2104496b0cdd79a29047021f8ee602a3f4f5caa316`
- v3 vs prior accepted compact80: exact match

Peak ingest RSS remains essentially flat from 100k through 1M and full 13 GB input.

## Repository gates

Before the full run:

- `cargo fmt --check`: PASS
- `git diff --check`: PASS
- `cargo test --workspace --locked`: PASS
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: PASS
- `cargo build --release --locked`: PASS

After the scratch-path fix:

- core tests: `379` green apart from existing explicitly ignored manual pilots
- all product/Inspector CLI suites: PASS
- Clippy with `-D warnings`: PASS
- release build: PASS

## Final patched product rerun

Patched binary `8488879555e11df2f4eade594661dca10a206ef05ea882007e8b6ab9bd9e1242` was run on the full Collection with no `TMPDIR` override.

- result: PASS, RC `0`
- scratch location: workspace `raw/tmp`
- system `/tmp`: unused by Antiskill duplicate scratch
- wall: `11m13.41s`
- user CPU: `626.33s`
- system CPU: `40.63s`
- maximum RSS: `68,356 KiB`
- byte-identical groups: `145`
- same-mainline/different-PGN groups: `28,266`
- non-comparable: `20,836`
- report bytes: `9,626,450`
- report lines: `126,878`
- report BLAKE3: `ddeb77dee4e275f8d8df87c70598a51967846ca10b00e31d663ad6fb6d07eddc`
- scratch after success: `0` bytes

The report hash exactly matches the prior successful full run with an explicit home-filesystem scratch directory.
A final patched-binary `doctor` also passed in `28.61s` with `6,196 KiB` maximum RSS.

## Acceptance conclusion

The original failure mode is resolved.

The baseline 100k product path was killed by the kernel after reaching about 10.58 GB RSS.
The accepted v3 path ingests the full `13,062,829,132`-byte source at about 17 MiB peak RSS,
preserves byte-exact SourceBlob custody, persists deterministic per-game semantic identity,
passes full doctor verification, supports byte-exact export and product search/duplicates,
and keeps stable memory as corpus size grows.

The old persistent 250k design took `8:39:19` for ingest and amplified disk by about `10.966x`.
The v3 full ingest completes in `5m19.52s`; stable storage is close to source size plus an
88-byte/game index.

All legacy issue 65 acceptance criteria are satisfied.
