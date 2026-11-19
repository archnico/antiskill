#!/usr/bin/env python3
"""Certify ONE independently sourced FSI↔FIDE ID pair with three-way evidence.

Required: (1) explicit Swiss-Manager/Chess-Results national+FIDE ID pair
from a tournament with established Italian FSI-ID semantics, (2) an official
FSI annual member lookup for that exact FSI ID, (3) current official FIDE
snapshot matching the explicit FIDE ID and name/birth year.

This deliberately rejects name-only matches, conflicting crosswalks, or
uncertain national-ID column semantics. No mass auto-certification.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import re
import sqlite3
from pathlib import Path

from fsi_roster_probe import parse_roster
from ingest import NOW, canonical_name

FSI_URL="https://www.federscacchi.com/fsi/index.php/struttura/tesserati"
DDL="""
CREATE TABLE IF NOT EXISTS externally_certified_crosswalk(
  fsi_id INTEGER NOT NULL,
  fide_id INTEGER NOT NULL,
  tournament_source_key TEXT NOT NULL,
  tournament_source_sha256 TEXT NOT NULL,
  official_fsi_source_key TEXT NOT NULL,
  official_fsi_source_sha256 TEXT NOT NULL,
  official_fide_source_key TEXT NOT NULL,
  official_fide_source_sha256 TEXT NOT NULL,
  certification_rule TEXT NOT NULL,
  checked_at TEXT NOT NULL,
  PRIMARY KEY(fsi_id,fide_id)
);
"""

def signature(raw):
    parts=canonical_name(raw).split()
    if len(parts)<2:
        raise ValueError("Too few name tokens for independent check")
    return " ".join(sorted(parts))

def certify(dbpath:Path, fsi_id:int, fide_id:int, roster:Path, year:int):
    if fsi_id<=0 or fide_id<=0 or not 2020<=year<=2100:
        raise ValueError("Invalid federation IDs or membership year")
    if roster.is_symlink() or not roster.is_file():
        raise ValueError("Official roster must be a regular local file")
    raw=roster.read_bytes()
    if not 20_000<=len(raw)<=2_000_000:
        raise ValueError("Unexpected official HTML size")
    # The FSI page must itself declare the requested membership year:
    # a historical roster cannot silently be relabeled as "current".
    declared_year=re.search(
        rb'<input\b[^>]*\bname\s*=\s*["\']?anno["\']?[^>]*\bvalue\s*=\s*["\']?(\d{4})',
        raw,re.I)
    if declared_year is None or int(declared_year.group(1))!=year:
        raise ValueError("Official FSI roster snapshot does not declare requested year")
    state,official_name,birthyear=parse_roster(raw,fsi_id)
    if state!="PARSED":
        raise ValueError("FSI official lookup does not contain the requested exact identity")
    digest=hashlib.sha256(raw).hexdigest()
    source_fsi=f"fsi-official-tesserato-{year}-{fsi_id}-{digest[:12]}"
    url=f"{FSI_URL}?anno={year}&idx={fsi_id}&ric=1"

    db=sqlite3.connect(dbpath)
    db.row_factory=sqlite3.Row
    try:
        db.executescript(DDL)
        # Lock and validate all source-based constraints before any INSERT.
        db.execute("BEGIN IMMEDIATE")
        fide=db.execute("SELECT fide_id,full_name,birth_year,source_key FROM fide_player WHERE fide_id=?",
                        (fide_id,)).fetchone()
        if not fide or not fide["birth_year"] or fide["source_key"]!="fide-current":
            raise ValueError("No official current FIDE identity for this ID")
        if signature(official_name)!=signature(fide["full_name"]) or birthyear!=fide["birth_year"]:
            raise ValueError("Official FSI and FIDE name/birth year disagreement")
        fide_source=db.execute("SELECT sha256 FROM source WHERE source_key=?",
                               (fide["source_key"],)).fetchone()
        if not fide_source or not re.fullmatch(r"[0-9a-f]{64}",fide_source["sha256"]):
            raise ValueError("Missing source fingerprint for current FIDE snapshot")
        ext=db.execute("""SELECT * FROM external_id_pair_observation
              WHERE fsi_id=? AND fide_id=? AND pair_status IN (
                'UNLINKED_ID_PAIR','AGREES_VERIFIED')
              ORDER BY source_key LIMIT 1""",(fsi_id,fide_id)).fetchone()
        if ext is None:
            raise ValueError("No explicit ID-pair source observation")
        if signature(ext["observed_name"])!=signature(official_name):
            raise ValueError("Chess-Results event name differs from official FSI")
        source_ext=db.execute("SELECT sha256 FROM source WHERE source_key=?",
                              (ext["source_key"],)).fetchone()
        if not source_ext or source_ext["sha256"]!=ext["source_sha256"]:
            raise ValueError("Tournament source fingerprint mismatch")
        if not ext["source_url"].startswith("https://s3.chess-results.com/tnr"):
            raise ValueError("Unknown tournament publication location")
        known,count=db.execute("""SELECT
            SUM(CASE WHEN pair_status='AGREES_VERIFIED' THEN 1 ELSE 0 END),
            COUNT(*) FROM external_id_pair_observation
            WHERE source_key=?""",(ext["source_key"],)).fetchone()
        if known is None or known<4 or known/count<0.80:
            raise ValueError("Independent page does not demonstrate FSI ID semantics")
        # Fail closed on conflicting national or international IDs in ALLIN and
        # on conflicting external ID-pair assertions for this national identity.
        other=db.execute("""SELECT fsi_id,fide_id,status FROM identity_link
            WHERE fsi_id=? OR fide_id=?""",(fsi_id,fide_id)).fetchall()
        if other and (len(other)!=1 or (other[0]["fsi_id"],other[0]["fide_id"],
                                         other[0]["status"])!=(fsi_id,fide_id,"VERIFIED")):
            raise ValueError("Conflicting existing crosswalk")
        conflicting_external=db.execute("""SELECT COUNT(*) FROM external_id_pair_observation
            WHERE (fsi_id=? AND fide_id<>?) OR (fide_id=? AND fsi_id<>?)""",
            (fsi_id,fide_id,fide_id,fsi_id)).fetchone()[0]
        if conflicting_external:
            raise ValueError("Conflicting external tournament ID assertions")
        prior=db.execute("SELECT * FROM fsi_player WHERE fsi_id=?",(fsi_id,)).fetchone()
        if prior and (signature(prior["full_name"])!=signature(official_name) or
                      prior["birth_year"] not in (None,birthyear)):
            raise ValueError("Official FSI lookup contradicts existing FSI player")
        exists=db.execute("SELECT * FROM externally_certified_crosswalk WHERE fsi_id=? AND fide_id=?",
                          (fsi_id,fide_id)).fetchone()
        if exists:
            if (exists["official_fsi_source_sha256"]!=digest or
                exists["tournament_source_sha256"]!=ext["source_sha256"] or
                exists["official_fide_source_sha256"]!=fide_source["sha256"]):
                raise ValueError("New evidence differs from existing certification")
            db.commit()
            return {"fsi_id":fsi_id,"fide_id":fide_id,"status":"ALREADY_CERTIFIED",
                    "new_verified_links":0}

        # This is the only writer for these verified crosswalks, and it requires
        # strong explicit IDs plus *independent official* FSI and FIDE checks.
        db.execute("INSERT OR IGNORE INTO source VALUES (?,?,?,?,?,?)",
            (source_fsi,url,digest,len(raw),NOW,
             "official FSI individual roster, manually selected source, internal"))
        if not prior:
            db.execute("""INSERT INTO fsi_player
              (fsi_id,full_name,birth_year,federation,category,latest_member_year,
               reported_fide_id,source_key)
              VALUES (?,?,?,?,?,?,?,?)""",
              (fsi_id,official_name,birthyear,None,None,year,None,source_fsi))
        if not other:
            db.execute("""INSERT INTO identity_link(fsi_id,fide_id,status,reason,source_key)
              VALUES (?,?,?,?,?)""",
              (fsi_id,fide_id,"VERIFIED",
               "official_fsi_and_fide_plus_explicit_chess_results_pair",
               ext["source_key"]))
        db.execute("INSERT INTO externally_certified_crosswalk VALUES (?,?,?,?,?,?,?,?,?,?)",
            (fsi_id,fide_id,ext["source_key"],ext["source_sha256"],
             source_fsi,digest,fide["source_key"],fide_source["sha256"],
             "manual_selection_three_source_exact_id_name_birthyear_v1",NOW))
        db.commit()
        if db.execute("PRAGMA integrity_check").fetchone()[0]!="ok":
            raise RuntimeError("Integrity check failure")
        return {"fsi_id":fsi_id,"fide_id":fide_id,"status":"VERIFIED_FROM_THREE_SOURCES",
                "new_verified_links":int(not other),
                "evidence":{"chess_results_sha256":ext["source_sha256"],
                            "official_fsi_sha256":digest,
                            "official_fide_sha256":fide_source["sha256"]},
                "note":"Manual selection, explicit national+FIDE IDs and two primary official identity records. No inference from name only."}
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
    result=certify(a.db,a.fsi_id,a.fide_id,a.fsi_roster,a.year)
    a.report.write_text(json.dumps(result,ensure_ascii=False,indent=2)+"\n")
    print(json.dumps(result,ensure_ascii=False,indent=2))
