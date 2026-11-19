#!/usr/bin/env python3
"""Promote official FSI club-roster *records* into private identity directory.

Never invents or infers FIDE IDs. Names and birth years are taken from saved
official FSI 2026 snapshots only. Existing ALLIN records are not overwritten.
"""
import argparse
import collections
import hashlib
import json
import sqlite3
from pathlib import Path
from ingest import canonical_name
from fsi_club_roster_probe import parse_club_roster

DDL="""
CREATE TABLE IF NOT EXISTS fsi_official_name_observation(
  fsi_id INTEGER NOT NULL,
  observed_year INTEGER NOT NULL,
  club_code INTEGER NOT NULL,
  full_name TEXT NOT NULL,
  birth_year INTEGER NOT NULL,
  source_key TEXT NOT NULL,
  source_sha256 TEXT NOT NULL,
  PRIMARY KEY(fsi_id,observed_year,club_code,source_sha256)
);
CREATE INDEX IF NOT EXISTS ix_fsi_official_name ON fsi_official_name_observation(fsi_id);
"""

def name_signature(s):
    return " ".join(sorted(canonical_name(s).split()))

def import_official(dbpath:Path,raw_dir:Path,report_path:Path,year=2026):
    if not 2020<=year<=2100:
        raise ValueError("Expected current official membership year")
    db=sqlite3.connect(dbpath)
    db.row_factory=sqlite3.Row
    try:
        db.executescript(DDL)
        snapshots=db.execute("""
          SELECT observed_year,club_code,source_key,source_sha256,parsed_members
          FROM fsi_official_club_roster_snapshot
          WHERE observed_year=? ORDER BY club_code""",(year,)).fetchall()
        if not snapshots:
            raise ValueError("No verified official FSI club snapshots")
        # Fail closed before any identity records are inserted. Raw source hashes
        # and year/club IDs must match previous observations.
        staged=[]
        appearances={}
        for src in snapshots:
            code=src["club_code"]
            local=raw_dir/f"fsi-club-roster-{year}-{code}.html"
            if local.is_symlink() or not local.is_file():
                raise ValueError("Missing or symlinked official club page")
            raw=local.read_bytes()
            digest=hashlib.sha256(raw).hexdigest()
            if digest!=src["source_sha256"]:
                raise ValueError("Official club source changed since acquisition")
            ref=db.execute("SELECT sha256 FROM source WHERE source_key=?",
                           (src["source_key"],)).fetchone()
            if ref is None or ref[0]!=digest:
                raise ValueError("Missing or mismatched acquisition source")
            members=parse_club_roster(raw,code,year)
            if len(members)!=src["parsed_members"]:
                raise ValueError("Official club members count changed")
            known={r[0] for r in db.execute(
                "SELECT fsi_id FROM fsi_official_club_roster WHERE club_code=? AND observed_year=? AND source_sha256=?",
                (code,year,digest))}
            if set(members)!=known:
                raise ValueError("Official source membership IDs changed")
            for fsi,item in members.items():
                previous=appearances.get(fsi)
                if previous and (
                    name_signature(previous["name"])!=name_signature(item["name"]) or
                    previous["birth_year"]!=item["birth_year"]):
                    raise ValueError("Conflicting FSI identities across official club pages")
                appearances[fsi]=item
            staged.append((src,members))
        counter=collections.Counter()
        db.execute("BEGIN IMMEDIATE")
        try:
            for src,members in staged:
                code=src["club_code"]; key=src["source_key"];sha=src["source_sha256"]
                for fsi,item in members.items():
                    person=db.execute("SELECT full_name,birth_year FROM fsi_player WHERE fsi_id=?",
                                      (fsi,)).fetchone()
                    if person is None:
                        db.execute("""INSERT INTO fsi_player
                          (fsi_id,full_name,birth_year,federation,category,latest_member_year,
                           reported_fide_id,source_key)
                          VALUES (?,?,?,NULL,NULL,?,NULL,?)""",
                          (fsi,item["name"],item["birth_year"],year,key))
                        counter["new_fsi_records"]+=1
                    elif (name_signature(person["full_name"])!=name_signature(item["name"])
                          or person["birth_year"] not in (None,item["birth_year"])):
                        # Keep both official and ALLIN observations; no silent overwrite.
                        counter["existing_name_or_year_differences"]+=1
                    db.execute("INSERT OR IGNORE INTO fsi_official_name_observation VALUES (?,?,?,?,?,?,?)",
                      (fsi,year,code,item["name"],item["birth_year"],key,sha))
                    before=db.total_changes
                    db.execute("INSERT OR IGNORE INTO club_membership VALUES (?,?,?,?,?)",
                      (fsi,code,year,"official_fsi_roster",key))
                    if db.total_changes>before:
                        counter["new_official_club_membership_observations"]+=1
            db.commit()
        except:
            db.rollback()
            raise
        if db.execute("PRAGMA integrity_check").fetchone()[0]!="ok":
            raise RuntimeError("SQLite integrity failure")
        result={
            "year":year,
            "official_club_rosters":len(snapshots),
            "official_people_observed":len(appearances),
            **dict(counter),
            "fide_links_created":0,
            "note":"The official FSI roster adds national identities and dated memberships, not FIDE identification."
        }
        report_path.write_text(json.dumps(result,ensure_ascii=False,indent=2)+"\n")
        return result
    finally:
        db.close()

if __name__=="__main__":
    p=argparse.ArgumentParser()
    p.add_argument("--db",type=Path,required=True)
    p.add_argument("--raw-dir",type=Path,required=True)
    p.add_argument("--report",type=Path,required=True)
    p.add_argument("--year",type=int,default=2026)
    a=p.parse_args()
    print(json.dumps(import_official(a.db,a.raw_dir,a.report,a.year),
                     indent=2,ensure_ascii=False))
