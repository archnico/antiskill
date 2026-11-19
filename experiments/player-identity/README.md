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
