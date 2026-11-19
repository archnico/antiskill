#!/usr/bin/env python3
"""Find still-unlinked Italian FSI/FIDE candidate IDs without creating identity links.

Strict candidate generation: same sorted normalized name tokens, same birth
year, unique on BOTH *full* FSI and FIDE-ITA lists, no prior FSI/FIDE assertion.
These are review leads, never proof of identity or an account attribution.
"""
import argparse
import collections
import hashlib
import json
import sqlite3
from pathlib import Path
from ingest import NOW, canonical_name

DDL="""
CREATE TABLE IF NOT EXISTS crosswalk_candidate (
  candidate_id TEXT PRIMARY KEY,
  fsi_id INTEGER NOT NULL,
  fide_id INTEGER NOT NULL,
  name_signature TEXT NOT NULL,
  birth_year INTEGER NOT NULL,
  fsi_source_sha256 TEXT NOT NULL,
  fide_source_sha256 TEXT NOT NULL,
  matching_rule TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status='CANDIDATE_ONLY'),
  observed_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_candidate_fsi ON crosswalk_candidate(fsi_id);
CREATE INDEX IF NOT EXISTS ix_candidate_fide ON crosswalk_candidate(fide_id);
"""

def candidate_key(name,birth_year):
    if birth_year is None or not (1850 <= int(birth_year) <= 2100):
        return None
    tokens=canonical_name(name).split()
    if len(tokens)<2:
        return None
    return (" ".join(sorted(tokens)),int(birth_year))

def calculate(db):
    db.row_factory=sqlite3.Row
    existing_fsi={row[0] for row in db.execute("SELECT DISTINCT fsi_id FROM fsi_fide_assertion")}
    existing_fide={row[0] for row in db.execute("SELECT DISTINCT fide_id FROM fsi_fide_assertion")}
    fsi=collections.defaultdict(list)
    fide=collections.defaultdict(list)
    for r in db.execute("SELECT fsi_id,full_name,birth_year FROM fsi_player"):
        k=candidate_key(r["full_name"],r["birth_year"])
        if k: fsi[k].append(int(r["fsi_id"]))
    for r in db.execute("SELECT fide_id,full_name,birth_year FROM fide_player WHERE fed='ITA'"):
        k=candidate_key(r["full_name"],r["birth_year"])
        if k: fide[k].append(int(r["fide_id"]))
    unique=[]
    ambiguous=0
    for key in sorted(set(fsi).intersection(fide)):
        ids_fsi=fsi[key]
        ids_fide=fide[key]
        # Evaluating uniqueness against the entire corpus prevents borrowing
        # an ID from a similarly named person who already has a bridge.
        if len(ids_fsi)!=1 or len(ids_fide)!=1:
            if any(i not in existing_fsi for i in ids_fsi) and any(i not in existing_fide for i in ids_fide):
                ambiguous+=1
            continue
        sid,fid=ids_fsi[0],ids_fide[0]
        if sid in existing_fsi or fid in existing_fide:
            continue
        unique.append((sid,fid,key[0],key[1]))
    return unique,ambiguous

def run(db_path:Path,report_path:Path):
    db=sqlite3.connect(db_path)
    try:
        db.executescript(DDL)
        sources={key:h for key,h in db.execute("SELECT source_key,sha256 FROM source")}
        # Candidate relationships require reproducible sources on both sides.
        source_fsi=sources.get("allin-2026-04")
        source_fide=sources.get("fide-current")
        if not source_fsi or not source_fide:
            raise RuntimeError("Need BOTH a source-backed FSI and current FIDE snapshot")
        candidates,ambiguous=calculate(db)
        db.execute("BEGIN IMMEDIATE")
        for sid,fid,name,birth_year in candidates:
            claim="|".join(map(str,(sid,fid,source_fsi,source_fide)))
            key=hashlib.sha256(claim.encode("utf-8")).hexdigest()
            db.execute("""INSERT OR IGNORE INTO crosswalk_candidate
              VALUES (?,?,?,?,?,?,?,?,?,?)""",
              (key,sid,fid,name,birth_year,source_fsi,source_fide,
               "exact_unique_name_tokens_and_birth_year_v1","CANDIDATE_ONLY",NOW))
        db.commit()
        if db.execute("PRAGMA integrity_check").fetchone()[0]!="ok":
            raise RuntimeError("Corrupt identity database")
        report={"candidate_pairs":len(candidates),
                "ambiguous_name_year_signatures":ambiguous,
                "fsi_source_sha256":source_fsi,
                "fide_source_sha256":source_fide,
                "source_scope":"FSI people in ALLIN and FIDE federation ITA in official current snapshot",
                "verified_links_created":0,
                "note":"Names+birthyear are NOT authority. All candidate pairs require independent evidence review."}
        report_path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
        return report
    except:
        db.rollback()
        raise
    finally:
        db.close()

if __name__=="__main__":
    p=argparse.ArgumentParser()
    p.add_argument("--db",type=Path,required=True)
    p.add_argument("--report",type=Path,required=True)
    a=p.parse_args()
    print(json.dumps(run(a.db,a.report),ensure_ascii=False,indent=2))
