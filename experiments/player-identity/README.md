# Player Identity Italia — experimental adapter

This is an **experimental source adapter and read-only identity dossier** for Antiskill issue [#19](https://github.com/archnico/antiskill/issues/19). It is not a new identity engine, a second PGN archive, or a central player directory. It does **not** change Antiskill core's game/occurrence IDs, deduplication, or preservation rules. It uses only Python 3's standard library.

**Scope:** candidate Italian FSI↔FIDE links, year-scoped club memberships and offices, and official FSI honour-roll observations. Raw third-party data stay in an operator-owned local workspace and are never included in this repository.

## Offline workflow

Download snapshots **only when access and terms permit**. Preserve original files and hashes. For example, source endpoints to evaluate:

- [Torneionline ALLIN](https://www.torneionline.com/dwn/allin.csv) (FSI ID, optional FIN/FIDE ID, reported club codes by year);
- [FIDE player list](https://ratings.fide.com/download_lists.phtml) (monthly official XML ZIP);
- [FSI societies](https://www.federscacchi.com/fsi/index.php/struttura/societa?anno=2026&ric=1) (historical year selected in URL);
- [FSI albi](https://www.federscacchi.com/fsi/index.php/eventi/albi-doro?albo=ALL&anno=0&chiave=&ricerca=Y).

Save snapshots outside the checkout, e.g. `$DATA/raw/`. Use a separately named output SQLite database: existing outputs are rebuilt, not incrementally updated.

```sh
cd experiments/player-identity
python3 -m unittest -v test_ingest

python3 ingest.py --allin "$DATA/raw/allin.csv" \
  --fide "$DATA/raw/players_list_xml.zip" \
  --out "$DATA/identity.sqlite3" --report "$DATA/ingest.json"

python3 enrich_clubs.py "$DATA/identity.sqlite3" \
  "$DATA/raw/fsi_societa_2026.html" "$DATA/clubs.json"

python3 enrich_club_history.py "$DATA/identity.sqlite3" \
  "$DATA/clubs-history.json" "$DATA"/raw/fsi_societa_20*.html

python3 enrich_all_albi.py "$DATA/identity.sqlite3" \
  "$DATA/raw/fsi_albi_completi.html" "$DATA/albi.json"

python3 player_lookup.py --db "$DATA/identity.sqlite3" --fsi-id 123456
python3 player_lookup.py --db "$DATA/identity.sqlite3" --fide-id 12345678
python3 player_lookup.py --db "$DATA/identity.sqlite3" --name "ROSSI MARIO"
```

To add the current FIDE list to an archive initially indexed against an **older** FIDE copy, use `update_current_fide.py DB CURRENT_ZIP REPORT`; it makes a separate SQLite backup and preserves existing office and award observations.

### Manual review queue

Export unresolved identity crosswalk assertions **only to the private local workspace**:

```sh
python3 review_queue.py --db "$DATA/identity.sqlite3" \
  --out-csv "$DATA/manual_review.csv" \
  --summary-json "$DATA/manual_review_summary.json"
```

The queue distinguishes inconsistent birth years, spelling changes, reordering of name tokens, and FIDE IDs missing from the checked snapshot. Its hints do **not** confirm or merge identities. The CSV includes names and years of birth; do not commit or redistribute it.

## Read-only discrepancy ledger (official evidence vs source assertions)

To audit the existing local pilot *without changing identities, PGN or source
snapshots*, run the offline reconciliation checker:

```sh
python3 source_discrepancies.py --db "$DATA/identity.sqlite3" \
  --summary-json "$DATA/reports/source-discrepancies-01.json" \
  --details-jsonl "$DATA/reports/source-discrepancies-01.jsonl"
```

Both outputs must be **new private files outside the public checkout**. The
source database opens read-only in one SQLite read transaction, so concurrent
ingestion does not silently mix two database states in one report. Results are
deterministic on an unchanged snapshot; outputs contain ID pairs, source keys,
SHA-256 provenance and reason codes, **not names or birth dates**. Keep even
ID-pair audit details private: the report is not a public player roster.

The decisive distinction is between:

- `COMPATIBLE_BUT_UNCERTIFIED`: an Allin pair agrees with a FIDE record or
  with some other observed facts; **this is not independent certification**.
  In particular, the legacy `identity_link.status=VERIFIED` is insufficient.
- `REVIEW_REQUIRED` / `CONFLICT`: missing IDs, different source assertions,
  mismatched names/years, conflicting pair observations, or impossible joins.
  Absence from a monthly list is not proof that a person/ID does not exist.
- `INDEPENDENTLY_CERTIFIED`: a separately certified three-source pair with
  matching source fingerprints and an explicit tournament ID pair.
- `INDEPENDENTLY_CERTIFIED_OUTSIDE_ALLIN`: the same independent proof,
  with the added *Allin source gap*. This is **not** a failed certificate.
- `CERTIFICATE_INVALID_REVIEW`: source hashes, explicit tournament pair,
  link status or certification rule do not validate; fail closed.

The JSON summary also reports how many FSI-only records were observed, how
many name/year matches exist in that *partial* coverage, and how many
`CANDIDATE_ONLY` pairs remain. These figures **must never be called complete
national totals**. The current FIDE snapshot filename and the current
acquisition date do not alone establish the effective publication period.

This audit intentionally **does not promote pairs**, write to
`identity_link`, duplicate the native PGN identity semantics or reimplement
the existing `review_queue.py`. The latter is a separate personal-data review
worksheet; this new report is a minimized evidence/discrepancy ledger.
Synthetic checks: `python3 -m unittest -v test_source_discrepancies`.

## Unlinked FSI ↔ FIDE candidates (no automatic merge)

The official ALLIN crosswalk leaves many people without an explicit FIDE ID.
After importing *both* ALLIN and a current FIDE list, produce a separate
reconciliation queue:

```sh
python3 crosswalk_candidates.py --db "$DATA/identity.sqlite3" \
  --report "$DATA/unlinked-candidates.json"
```

The conservative rule requires (1) equal normalized name tokens, (2) the
same nonmissing birth year, (3) uniqueness against the **whole** FSI and
FIDE-ITA directories, and (4) no pre-existing FSI/FIDE pairing. Results are
stored with status `CANDIDATE_ONLY`. **Name+birth year are not identity
proof**: no `identity_link` record, PGN attribution or online-account link
is created. Foreign-federation FIDE players with Italian FSI history can
remain outside this narrow candidate-generation scope.

## Online account candidates and PGN identity overlay

Antiskill must not infer a real person from an online username, a chess rating, a
club membership, or an unverified social profile. Fide-Scout account observations
can be imported **as candidates only** from an existing local SQLite database:

```sh
python3 online_candidates.py --db "$DATA/identity.sqlite3" \
  --fide-scout-db "$DATA/fide-scout.sqlite3" \
  --report "$DATA/online_candidates.json"
```

No network calls, no silent `VERIFIED` promotion (even if the upstream source
says `VERIFIED` or `HIGH`); the canonical SQLite SQL-dump hash is stored as
provenance. In FSI dossiers, account candidates are shown only through a
previously verified FSI/FIDE ID bridge; they are still labeled **unconfirmed**.

For an existing **Antiskill workspace that has ingested PGN**, use the core's
own `players --format json` command and our strictly read-only overlay:

```sh
python3 core_overlay.py --db "$DATA/identity.sqlite3" \
  --antiskill-bin /path/to/antiskill --workspace /path/to/workspace \
  --fide-id 900000001 --report "$DATA/player-core-overlay.json"
```

Add `--federation-index /path/to/native-ALLIN-index.jsonl` when an index
generated by Antiskill's own `sources index torneionline-allin` is available.
`--context-registry` is also accepted by the native command. The overlay
rejects a conflicting FSI ID from the existing Nucleo; no PGN is rewritten,
no game is deleted, and no algorithm for duplicate moves or game identity is
duplicated. Two games with the same mainline but distinct tournament/date
remain two distinct occurrences.

**Limit:** this is a tested experimental bridge, not an integrated permanent
`antiskill player dossier` CLI. Connecting approved online accounts and
official tournament records to games is a later evidence-gated task.

## Official FSI roster checks and durable PGN attribution

The official FSI [tesserati](https://www.federscacchi.com/fsi/index.php/struttura/tesserati)
directory can corroborate an **FSI** name/birth-year independently of ALLIN.
It does **not** itself attest any `fide_id`. Avoid turning a corroborated
FSI identity into a FSI/FIDE identity merge.

A bounded initial pilot fetches at most 12 individual FSI pages (adult
2026 candidates only). The efficient path instead fetches at most five
official *club roster* pages per invocation, each containing many FSI ID
observations. They are kept under a private path with source hashes:

```sh
python3 fsi_roster_probe.py --db "$DATA/identity.sqlite3" \
  --raw-dir "$DATA/raw/fsi-individual-checks" \
  --report "$DATA/fsi-individual-probe.json" --max-checks 8

python3 fsi_club_roster_probe.py --db "$DATA/identity.sqlite3" \
  --raw-dir "$DATA/raw/fsi-club-rosters" \
  --report "$DATA/fsi-clubs-probe.json" --max-clubs 4
```

To review the **entire** unlinked crosswalk locally without further
network requests, rank independent FSI checks first:

```sh
python3 candidate_review_queue.py --db "$DATA/identity.sqlite3" \
  --out-csv "$DATA/candidates_to_review.csv" \
  --summary-json "$DATA/candidate_review_summary.json"
```

The queue is private, CSV-formula-safe, sorted by conflict/corroboration
priority and retains both original source hashes. Rows with corroborated
FSI official names are **still unverified as FSI↔FIDE pairs**.

Both tools have enforced bounded network requests and a minimum delay; parsing
fails closed on count, club, year and ID disagreement. Even a
`FSI_ROSTER_CORROBORATED` observation means **only the FSI record has been
independently checked**. No candidate pair is automatically promoted.

An Antiskill `players` response can also be **persisted** into an
independent evidence ledger, after generating a core overlay:

```sh
python3 persist_core_occurrences.py --db "$DATA/identity.sqlite3" \
  --overlay "$DATA/player-core-overlay.json" \
  --report "$DATA/persisted-occurrences.json"
```

The ledger records `collection_blake3`, native `g1-...` occurrence IDs,
player side, exact raw source ranges, FIDE/FSI IDs, crosswalk source hash
and evidence class. Only a **single, conflict-free, corroborated** FSI/FIDE
link is accepted; candidate-only links are rejected. The ledger is
transactional and idempotent, rejecting contradictory re-attributions and
never replacing the Nucleo's deduplication or game metadata.

This is an evidence-bearing local projection, not a rewrite of the existing
`game_attribution` experimental table, which lacks collection/side scope.
Do not publish roster snapshots (including minors' data), real names,
account candidates or raw PGNs in the public repository.

## Third-party explicit FSI ↔ FIDE ID pairs

Some Italian tournament tables on [Chess-Results](https://chess-results.com/)
show a national player ID and the FIDE ID **on the same row**.
The service is a publisher distinct from Torneionline; its underlying
registration data can still be shared. This evidence is never treated as
independent solely because the domain differs.

First save a small, authorized tournament snapshot in the **private** raw
directory. The parser recognizes the `ID` and `ID FIDE` columns only if
a strong majority (at least 80%, with at least four examples) matches
independently known FSI/FIDE pairs. This avoids interpreting a foreign
federation's national IDs as FSI identifiers.

```sh
python3 chess_results_pairs.py \
  --db "$DATA/identity.sqlite3" \
  --tournament-id 1234567 \
  --html "$DATA/raw/chess-results/tnr1234567.html" \
  --report "$DATA/chess-results-1234567.json"
```

The first phase stores only `external_id_pair_observation`, including
publication URL, exact ID pair and SHA-256 source hash. It never merges IDs.
The statuses distinguish prior corroboration, conflicts and previously
unlinked pairs.

**Certification of one specifically reviewed pair** requires all of:
an explicit pair in the validated tournament table, an independently
retrieved FSI membership page for that exact FSI ID and year, a current
official FIDE record for that exact FIDE ID, compatible names and birth
years across all three, and no contradictory mappings. This is a
deliberately singular, non-batch operation:

```sh
python3 certify_external_pair.py --db "$DATA/identity.sqlite3" \
  --fsi-id 900001 --fide-id 900000001 \
  --fsi-roster "$DATA/raw/fsi-member-900001.html" \
  --year 2026 --report "$DATA/certified-pair-900001.json"
```

Only this three-way proof may create a new `VERIFIED` link (with a
separate audit trail in `externally_certified_crosswalk`). A second run
must be idempotent. A name/year-only candidate never qualifies.

### Fill holes in ALLIN with official FSI membership records

The FSI club-roster snapshots may include valid FSI IDs absent from the
dated ALLIN snapshot. After collecting and validating them, ingest **only
officially published national identities and dated club memberships**:

```sh
python3 fsi_official_roster_ingest.py --db "$DATA/identity.sqlite3" \
  --raw-dir "$DATA/raw/fsi-club-rosters" \
  --year 2026 --report "$DATA/fsi-national-roster-ingest.json"
```

Existing names are never overwritten. Discrepancies are recorded, and
`reported_fide_id` stays NULL for official FSI-only additions. This
private source contains minors' records: **do not commit or redistribute
the pages, CSV exports or derived player database**.

Rerun `crosswalk_candidates.py` afterward: candidates now retain the
actual **per-player** FSI source fingerprint, not a blanket ALLIN
fingerprint. A pair certified by explicit IDs is retired from the
name/year-only candidate queue with an auditable retirement record;
other invalidated candidates fail closed pending review.

## Evidence semantics

| Status | Meaning |
|---|---|
| `VERIFIED` | Direct FSI↔FIDE source assertion corroborated by latest FIDE ID and compatible name/birth-year fields; not a general identity certificate |
| `REVIEW` | Official IDs matched but name spelling or birth year conflicted; preserve rather than merge |
| `SOURCE_ASSERTED` | FSI↔FIDE ID pair reported by ALLIN, not found in the checked FIDE snapshot |
| `CONFLICT` | Non-bijective ID mapping; manual review required |
| `HISTORICAL_MATCH` | Corroboration only against a dated local FIDE archive |
| `CANDIDATE_NAME_ONLY` | Honour-roll name has a unique candidate among FSI records; **not verified** |
| `CANDIDATE` | Office name and club-year coincide with FSI data; **not verified** |
| `AMBIGUOUS`, `UNLINKED` | Preserve separate mentions; never guess or discard |

Team / school honours are expressly **not individual player wins**; institutional awards likewise remain separate. Different historical events may happen to contain an identical move sequence, and this adapter does not collapse them.

### Known gaps and restrictions

- ALLIN is not a complete census of all historical or current Italian chess players. Its update date is not the retrieval date.
- The published FIDE federation code `ITA` denotes current federation, not citizenship or lifelong identity. Historical federation transfers must remain traceable.
- Name equality is insufficient to resolve a person: neither a tournament winner nor a social account is attributed automatically.
- Year of birth is retained only as a matching signal, never full birth dates. Personal contact and address fields are intentionally excluded. Exercise additional care with minors.
- Parsing is bounded to the observed FSI HTML/ALLIN formats and should fail closed on unexpected inputs. No active network requests occur in these offline scripts.
- Raw FSI/FIDE/ALLIN data default to rights class `internal` until an explicit redistribution license is established.
- A release-quality implementation must use Antiskill's shared evidence/identity contracts rather than elevate this experimental SQLite schema into a parallel Nucleo.

## Development gates

The synthetic tests include same-name identities, contradictory official-ID assertions, name/date discrepancies, individual vs team honours, and absence of sensitive fields. No real player lists or PGNs are committed. Use this experiment to specify a reviewed shared identity contract and make a **separate** integration PR when the adapter is stable.
