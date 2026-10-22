# Legacy development issue 85 — canonical unique-game view v1 acceptance (2026-09-11)

## Result

Legacy development issue 85 adds a non-destructive logical-game layer over the verified Collection v3 corpus.
The accepted implementation keeps every byte-exact source occurrence and derives logical identity
from the already frozen canonical-mainline identity; non-comparable occurrences remain distinct singletons.

The frozen full-corpus acceptance dataset contains:

- source occurrences: `13,058,194`
- logical unique games: `13,028,122`
- collapsed redundant occurrences: `30,072`
- comparable occurrences: `13,037,358`
- non-comparable singleton logical games: `20,836`

No SourceBlob or original occurrence is deleted, rewritten, or replaced.

## Code provenance

- implementation commit: `e90c6ec80f281703bc249d96c9b6bd2cfc1a70c5`
- commit subject: `feat(inspector): add canonical unique-game view`
- DCO sign-off: present on the implementation commit
- accepted release binary BLAKE3: `3fbb3778cdc8d0a43441b44dacd67d9199f7f4e3660d3acf0ee7f8e6da29a645`
- source archive BLAKE3 used to reproduce the commit: `06d1bec5111fe6ec4b69c4f1efb2f3038741b45659c6e6c80378edde8cbdeba2`

## Identity and representation contract

Comparable occurrences are grouped **only** by the existing verified `canonical_mainline_id`.
The public logical-game identity is a versioned deterministic `u1-*` identity derived from that canonical identity.
For a non-comparable occurrence, the fallback `u1-*` identity is derived from its public occurrence identity;
no equivalence is guessed.

For every multi-occurrence logical game the persistent sidecar records:

- logical-game `u1-*` identity;
- canonical-mainline identity;
- deterministic representative occurrence (lowest public occurrence ID);
- complete sorted list of supporting `g1-*` occurrence IDs;
- occurrence count;
- Collection identity and path.

Singletons are not redundantly materialized in the persistent sidecar. `antiskill unique-games --list`
provides a deterministic streaming JSONL listing of **all** logical games, including singleton records,
without building a second 13-million-row database.
## Full construction

`antiskill unique-games --format json` on the frozen full Collection completed successfully.

- source occurrences: `13,058,194`
- comparable occurrences: `13,037,358`
- comparable logical games: `13,007,286`
- non-comparable singleton logical games: `20,836`
- total logical games: `13,028,122`
- collapsed occurrences: `30,072`
- multi-occurrence logical games: `28,411`
- occurrences in multi-occurrence groups: `58,483`
- maximum occurrences supporting one logical game: `529`
- first-run wall: `1m27.14s`
- first-run maximum RSS: `39,652 KiB`
- second-run wall: `1m25.12s`
- second-run maximum RSS: `41,072 KiB`
- workspace scratch after each successful run: `0` bytes

The sidecar arithmetic was independently checked: `58,483 - 28,411 = 30,072`.
All `28,411` logical IDs and canonical-mainline IDs in the sidecar are unique.
## Persistent artifacts and storage

The content-addressed artifacts are:

- Unique Game View logical BLAKE3: `44251b9858f472a067c435a1f2959233eff8f477d52bff0ba02b3dd4977bc404`
- Unique Game View logical bytes: `1,237`
- Unique Game Groups logical BLAKE3: `fdac6281a1e0b6221ca42cbeb01feca1c55303955a2f9d62e21504588d78a938`
- Unique Game Groups logical bytes: `13,242,507`
- compressed view bytes on disk: `572`
- compressed groups bytes on disk: `4,565,556`

The stable workspace grew from `14,211,951,062` to `14,216,517,190` bytes,
only `4,566,128` bytes of persistent overhead. The original `13,062,829,132`-byte
SourceBlob remains unchanged and authoritative.

Repeated construction produced the same counts and the same two content hashes,
without publishing duplicate artifacts.
## Full streaming listing

`antiskill unique-games --list` was also exercised over the full corpus without persisting the stream.

- output lines: `13,028,124`
- expected structure: one header + `13,028,122` logical-game records + one footer
- final footer: `complete:true`
- footer source occurrences: `13,058,194`
- footer logical games: `13,028,122`
- full stream BLAKE3: `a18d5421cd1562429ceec9c000298a73f10266c06b682b4288c73732ab6d50f0`
- wall: `1m48.11s`
- maximum RSS: `22,048 KiB`
- exit status: `0`
- workspace scratch after completion: `0` bytes

The terminal completeness record makes a truncated stream distinguishable from a complete export.
The listing exposes every logical game, including non-comparable singleton entries, without
materializing a persistent 13-million-record listing.
## Verification and failure behavior

The dedicated CLI tests cover:

- canonical-mainline grouping with a synthetic `3 occurrences -> 2 logical games` corpus;
- deterministic repeated construction and content identities;
- non-comparable singleton preservation;
- complete JSONL listing shape and terminal completeness record;
- fail-closed behavior after deliberate SourceIndex corruption;
- no final view published after corrupted evidence;
- workspace-owned scratch empty after both success and the induced failure.

The persistent sidecar was decompressed and re-hashed independently; its logical BLAKE3 and
logical byte count match the product summary exactly.
## Repository and independent-host gates

On the primary Linux validation host, the implementation passed:

- `cargo fmt --check`
- `git diff --check`
- `cargo test --workspace --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo build --release --locked`

The exact implementation commit `e90c6ec80f281703bc249d96c9b6bd2cfc1a70c5` was also validated
independently on a second Linux host with an isolated Rust `1.98.0` toolchain. The same fmt, full test,
Clippy and release-build gates passed there. No global toolchain change was required.

The full-corpus acceptance itself used the release binary built from the same frozen commit on the corpus host.
## Acceptance conclusion

Legacy development issue 85 satisfies the intended non-destructive deduplication contract.
Antiskill still preserves all `13,058,194` byte-exact source occurrences, while the verified
logical corpus contains `13,028,122` unique games. Exactly `30,072` redundant occurrences are
collapsed in the logical view, not deleted from archival custody.

Construction is deterministic, bounded-memory, workspace-local for scratch, fail-closed on
corrupted evidence, and adds only about `4.57 MB` of stable compressed storage to the accepted
full workspace. Every logical game can be enumerated with deterministic public identity and
complete provenance back to its supporting occurrence IDs.

Subject to repository CI on Linux, macOS and Windows, all legacy issue 85 acceptance criteria are met.
