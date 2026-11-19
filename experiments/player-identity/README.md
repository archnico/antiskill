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
