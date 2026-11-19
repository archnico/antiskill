#!/usr/bin/env python3
"""Archive conflicting FSI/FIDE birth-year evidence without reclassifying IDs.

Never silently overwrite a FIDE birth year, an FSI birth year or the existing
FSI↔FIDE REVIEW status. Records source SHA-256 and original per-ID URLs.
"""
import argparse
import hashlib
import json
import re
import sqlite3
from pathlib import Path

from fsi_roster_probe import parse_roster, ROSTER
from ingest import NOW,canonical_name

DDL="""
CREATE TABLE IF NOT EXISTS crosswalk_conflict_evidence(
 fsi_id INTEGER NOT NULL,
 fide_id INTEGER NOT NULL,
 fsi_birth_year INTEGER NOT NULL,
 fide_birth_year INTEGER NOT NULL,
 conflict_type TEXT NOT NULL CHECK(conflict_type IN
 ('BIRTH_YEAR_DISAGREEMENT','NAME_DISAGREEMENT')),
 chess_results_source_key TEXT NOT NULL,
 chess_results_sha256 TEXT NOT NULL,
 fsi_roster_sha256 TEXT NOT NULL,
 fide_snapshot_sha256 TEXT NOT NULL,
 fsi_roster_url TEXT NOT NULL,
 observed_at TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status='REVIEW_REQUIRED'),
 PRIMARY KEY(fsi_id,fide_id,fsi_roster_sha256,fide_snapshot_sha256)
);
"""

def signature(name):
    return " ".join(sorted(canonical_name(name).split()))

def record_conflict(dbpath:Path,fsi_id:int,fide_id:int,roster:Path,year:int,report:Path):
    if fsi_id<=0 or fide_id<=0:
        raise ValueError("Nonpositive ID")
    if roster.is_symlink() or not roster.is_file():
        raise ValueError("No regular official source HTML")
    raw=roster.read_bytes()
    official=parse_roster(raw,fsi_id)
    if official[0]!="PARSED":
        raise ValueError("FSI official source not parsable")
    expected_year=re.search(
      rb'<input\b[^>]*\bname\s*=\s*["\']?anno["\']?[^>]*\bvalue\s*=\s*["\']?(\d{4})',
      raw,re.I)
    if not expected_year or int(expected_year.group(1))!=year:
        raise ValueError("FSI roster year differs")
    _,name_fsi,birth_fsi=official
    hash_fsi=hashlib.sha256(raw).hexdigest()
    db=sqlite3.connect(dbpath)
    db.row_factory=sqlite3.Row
    try:
        db.executescript(DDL)
        db.execute("BEGIN IMMEDIATE")
        link=db.execute("""SELECT * FROM identity_link WHERE fsi_id=? AND fide_id=?""",
                        (fsi_id,fide_id)).fetchone()
        if link is None or link["status"]!="REVIEW":
            raise ValueError("Only existing REVIEW pair may be logged")
        if db.execute("""SELECT count(*) FROM identity_link
                  WHERE (fsi_id=? AND fide_id<>?) OR (fide_id=? AND fsi_id<>?)""",
                  (fsi_id,fide_id,fide_id,fsi_id)).fetchone()[0]:
            raise ValueError("Conflicting national or FIDE ID assignment")
        fide=db.execute("SELECT full_name,birth_year,source_key FROM fide_player WHERE fide_id=?",
                        (fide_id,)).fetchone()
        if fide is None or fide["birth_year"] is None or fide["source_key"]!="fide-current":
            raise ValueError("Missing current official FIDE ID")
        fhash=db.execute("SELECT sha256 FROM source WHERE source_key='fide-current'").fetchone()
        if not fhash or not re.fullmatch(r"[0-9a-f]{64}",fhash["sha256"]):
            raise ValueError("FIDE official source missing")
        ext=db.execute("""SELECT source_key,source_sha256 FROM external_id_pair_observation
           WHERE fsi_id=? AND fide_id=? AND pair_status='AGREES_DISPUTED'
           ORDER BY source_key LIMIT 1""",(fsi_id,fide_id)).fetchone()
        if ext is None:
            raise ValueError("No explicit third-party conflicting pair source")
        chash=db.execute("SELECT sha256 FROM source WHERE source_key=?",
                         (ext["source_key"],)).fetchone()
        if not chash or chash["sha256"]!=ext["source_sha256"]:
            raise ValueError("Source hash collision")
        if birth_fsi!=fide["birth_year"]:
            conflict="BIRTH_YEAR_DISAGREEMENT"
        elif signature(name_fsi)!=signature(fide["full_name"]):
            conflict="NAME_DISAGREEMENT"
        else:
            raise ValueError("No discrepancy to investigate")
        official_url=(f"{ROSTER}?anno={year}&idx={fsi_id}&ric=1")
        db.execute("INSERT OR IGNORE INTO crosswalk_conflict_evidence VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
            (fsi_id,fide_id,birth_fsi,fide["birth_year"],conflict,
             ext["source_key"],ext["source_sha256"],hash_fsi,
             fhash["sha256"],official_url,NOW,"REVIEW_REQUIRED"))
        db.commit()
        result={"fsi_id":fsi_id,"fide_id":fide_id,"conflict_type":conflict,
                "status":"REVIEW_REQUIRED",
                "fsi_official_birth_year":birth_fsi,
                "fide_official_birth_year":fide["birth_year"],
                "evidence_hashes":{
                  "chess_results":ext["source_sha256"],
                  "fsi_roster":hash_fsi,
                  "fide_official":fhash["sha256"]},
                "verified_link_created":0,
                "original_review_status_preserved":True}
        report.write_text(json.dumps(result,ensure_ascii=False,indent=2)+"\n")
        return result
    except:
        db.rollback()
        raise
    finally:
        db.close()

if __name__=="__main__":
    p=argparse.ArgumentParser()
    p.add_argument("--db",type=Path,required=True)
    p.add_argument("--fsi-id",type=int,required=True)
    p.add_argument("--fide-id",type=int,required=True)
    p.add_argument("--fsi-roster",type=Path,required=True)
    p.add_argument("--year",type=int,default=2026)
    p.add_argument("--report",type=Path,required=True)
    a=p.parse_args()
    print(json.dumps(record_conflict(a.db,a.fsi_id,a.fide_id,a.fsi_roster,a.year,a.report),
                     indent=2,ensure_ascii=False))
