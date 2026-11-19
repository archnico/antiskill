#!/usr/bin/env python3
"""Import all official FSI honour rolls as source observations, never proof of identity.

Only individual awards may nominate candidate FSI identities, by an exact,
order-invariant normalized name; no automatic player merge or verified person link.
Teams, schools, institutional awards are stored but NEVER treated as player wins.
"""
from __future__ import annotations
import collections
import datetime as dt
import hashlib
import html
import json
import re
import sqlite3
import sys
from pathlib import Path
from ingest import NOW, canonical_name, sha

SOURCE_URL = "https://www.federscacchi.com/fsi/index.php/eventi/albi-doro?albo=ALL&anno=0&chiave=&ricerca=Y"
SOURCE_KEY_PREFIX = "fsi-all-honour-rolls"

def visible(markup):
    markup = re.sub(r"(?i)<br\s*/?>", " ", markup)
    return " ".join(html.unescape(re.sub(r"<[^>]+>", " ", markup)).split())

def parse_honour_rolls(raw: bytes):
    content = raw.decode("utf-8", errors="replace")
    rows = []
    for match in re.finditer(r"(?is)<tr(?:\s+[^>]*)?>(.*?)</tr>", content):
        cells = [visible(c) for c in re.findall(
            r"(?is)<t[dh](?:\s+[^>]*)?>(.*?)</t[dh]>", match.group(1))]
        if len(cells) != 7 or not cells[2].isdigit():
            continue
        year = int(cells[2])
        if not (1850 <= year <= dt.datetime.now().year):
            continue
        if not cells[0]:
            continue
        rows.append({"event": cells[0], "edition": cells[1], "year": year,
                     "gold": cells[3], "silver": cells[4], "bronze": cells[5],
                     "location": cells[6]})
    if len(rows) < 100:
        raise ValueError(f"FSI honour roll unexpectedly small: {len(rows)} rows")
    return rows

def classify(event):
    x = canonical_name(event)
    if any(t in x for t in ("squadre", "squadra", "tss", "cgs", "scuole")):
        return "team_or_school"
    if any(t in x for t in ("benemerenza", "premio", "citta amica")):
        return "institutional"
    if "on line" in x or "online" in x:
        return "online_individual"
    return "individual"

def import_all(db_path: Path, html_path: Path, report_path: Path):
    data = html_path.read_bytes()
    items = parse_honour_rolls(data)
    digest = sha(html_path)
    source_key = f"{SOURCE_KEY_PREFIX}-{digest[:12]}"
    db = sqlite3.connect(db_path)
    try:
        # All new awards are separate source observations (an event name is not a person).
        db.executescript("""
          CREATE TABLE IF NOT EXISTS honour_roll_entry (
            entry_id TEXT PRIMARY KEY,
            event_name TEXT NOT NULL,
            event_year INTEGER NOT NULL,
            edition TEXT NOT NULL,
            classification TEXT NOT NULL,
            location TEXT,
            source_key TEXT NOT NULL,
            source_sha256 TEXT NOT NULL
          );
          CREATE TABLE IF NOT EXISTS honour_roll_placement (
            entry_id TEXT NOT NULL,
            placement INTEGER NOT NULL CHECK(placement BETWEEN 1 AND 3),
            original_label TEXT NOT NULL,
            candidate_fsi_id INTEGER,
            link_status TEXT NOT NULL,
            source_key TEXT NOT NULL,
            PRIMARY KEY(entry_id, placement),
            FOREIGN KEY(entry_id) REFERENCES honour_roll_entry(entry_id)
          );
          CREATE INDEX IF NOT EXISTS idx_roll_year ON honour_roll_entry(event_year);
          CREATE INDEX IF NOT EXISTS idx_roll_class ON honour_roll_entry(classification);
          CREATE INDEX IF NOT EXISTS idx_roll_person ON honour_roll_placement(candidate_fsi_id);
        """)
        known = collections.defaultdict(set)
        for fsi, name in db.execute("SELECT fsi_id, full_name FROM fsi_player"):
            key = tuple(sorted(canonical_name(name).split()))
            if key: known[key].add(fsi)
        db.execute("BEGIN")
        db.execute("INSERT OR IGNORE INTO source VALUES (?,?,?,?,?,?)",
                (source_key, SOURCE_URL, digest, len(data), NOW,
                 "source FSI official all-year honour roll, NOT individual identity attestation"))
        tally = collections.Counter()
        tally["source_rows"] = len(items)
        for idx, entry in enumerate(items):
            label = entry["event"]
            group = classify(label)
            tally[group] += 1
            stable_key = json.dumps([label, entry["year"], entry["edition"],
                                     entry["location"], entry["gold"], entry["silver"], entry["bronze"]],
                                    ensure_ascii=False, separators=(",", ":"))
            entry_id = "fsi-roll-" + hashlib.sha256(stable_key.encode()).hexdigest()[:24]
            db.execute("INSERT OR IGNORE INTO honour_roll_entry VALUES (?,?,?,?,?,?,?,?)",
                    (entry_id, label, entry["year"], entry["edition"], group,
                     entry["location"], source_key, digest))
            for rank, field in [(1, "gold"), (2, "silver"), (3, "bronze")]:
                person_label = entry[field]
                if not person_label or person_label in ("-", "—"):
                    continue
                if group=="team_or_school":
                    candidate, status = None, "NOT_PERSON_TEAM_OR_SCHOOL"
                elif group=="institutional":
                    candidate, status = None, "NOT_PLAYER_WIN_INSTITUTIONAL"
                else:
                    k = tuple(sorted(canonical_name(person_label).split()))
                    matches = known.get(k, set())
                    if len(matches)==1:
                        candidate, status = next(iter(matches)), "CANDIDATE_NAME_ONLY"
                    elif len(matches)>1:
                        candidate, status = None, "AMBIGUOUS"
                    else:
                        candidate, status = None, "UNLINKED"
                db.execute("INSERT OR IGNORE INTO honour_roll_placement VALUES (?,?,?,?,?,?)",
                        (entry_id, rank, person_label, candidate, status, source_key))
                tally[f"placement_{rank}"] += 1
                tally[f"status_{status}"] += 1
                if group=="online_individual" and rank==1:
                    tally["online_champion_mentions"] += 1
        db.commit()
        if db.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
            raise ValueError("Integrity failure in updated database")
        report = {"source_url": SOURCE_URL, "sha256": digest,
                  "notes": "Source includes player, team, school and institutional results; title mentions are not always unique identities",
                  "metrics": dict(tally),
                  "entries_stored": db.execute("SELECT count(*) FROM honour_roll_entry").fetchone()[0],
                  "placements_stored": db.execute("SELECT count(*) FROM honour_roll_placement").fetchone()[0]}
        report_path.write_text(json.dumps(report, indent=2, ensure_ascii=False)+"\n")
        return report
    except Exception:
        db.rollback()
        raise
    finally:
        db.close()

if __name__=="__main__":
    if len(sys.argv) != 4:
        raise SystemExit("Usage: enrich_all_albi.py DATABASE RAW_HTML REPORT_JSON")
    print(json.dumps(import_all(*map(Path,sys.argv[1:])),indent=2,ensure_ascii=False))
