#!/usr/bin/env python3
"""Persist Antiskill's authoritative FIDE PGN player occurrences in a scoped ledger.

Does NOT parse PGN, merge games, or infer a person from an unverified name.
Core collection hash, side and source offsets prevent cross-corpus collisions.
"""
import argparse
import json
import re
import sqlite3
from pathlib import Path
from core_overlay import HEX64, GAME

DDL="""
CREATE TABLE IF NOT EXISTS verified_core_player_occurrence (
  collection_blake3 TEXT NOT NULL,
  occurrence_id TEXT NOT NULL,
  player_side TEXT NOT NULL CHECK(player_side IN ('white','black')),
  fide_id INTEGER NOT NULL,
  fsi_id INTEGER NOT NULL,
  source_blake3 TEXT NOT NULL,
  source_bytes INTEGER NOT NULL,
  source_start INTEGER NOT NULL,
  source_end INTEGER NOT NULL,
  ordinal INTEGER NOT NULL,
  core_profile TEXT NOT NULL,
  crosswalk_source_key TEXT NOT NULL,
  crosswalk_sha256 TEXT NOT NULL,
  evidence_class TEXT NOT NULL,
  PRIMARY KEY(collection_blake3,occurrence_id,player_side)
);
CREATE INDEX IF NOT EXISTS ix_core_fide ON verified_core_player_occurrence(fide_id);
CREATE INDEX IF NOT EXISTS ix_core_fsi ON verified_core_player_occurrence(fsi_id);
"""

def materialize(db, overlay):
    if overlay.get("profile")!="antiskill_player_identity_overlay_v1":
        raise ValueError("Unknown overlay profile")
    if overlay.get("fsi_relation")!="VERIFIED_ID_CROSSWALK":
        raise ValueError("Refuse to attribute unresolved FSI identity")
    fsi=overlay.get("fsi_id")
    fide=overlay.get("fide_id")
    if not isinstance(fsi,int) or fsi<=0 or not str(fide).isdigit() or int(fide)<=0:
        raise ValueError("Missing identity IDs")
    fide=int(fide)
    coll=overlay.get("core_collection_blake3")
    if not isinstance(coll,str) or not HEX64.fullmatch(coll):
        raise ValueError("Invalid collection BLAKE3")
    occurrences=overlay.get("core_occurrences")
    if not isinstance(occurrences,list) or len(occurrences)!=overlay.get("core_occurrence_count"):
        raise ValueError("Invalid authoritative occurrence count")
    evidence=overlay.get("fsi_crosswalk_evidence")
    if not isinstance(evidence,list) or len(evidence)!=1:
        raise ValueError("Multiple or absent crosswalk evidence rows")
    link=evidence[0]
    if (link.get("fsi_id"),link.get("fide_id"),link.get("status"))!=(fsi,fide,"VERIFIED"):
        raise ValueError("Overlay crosswalk evidence disagrees with IDs")
    db.row_factory=sqlite3.Row
    actual=[dict(r) for r in db.execute(
        "SELECT fsi_id,fide_id,status,source_key FROM identity_link WHERE fide_id=?",(fide,))]
    if len(actual)!=1 or actual[0]["status"]!="VERIFIED" or actual[0]["fsi_id"]!=fsi:
        raise ValueError("Identity crosswalk is no longer unique and verified")
    others=db.execute("SELECT fide_id,status FROM identity_link WHERE fsi_id=?",(fsi,)).fetchall()
    if len(others)!=1 or others[0]["fide_id"]!=fide or others[0]["status"]!="VERIFIED":
        raise ValueError("FSI identity has conflicting or additional FIDE assertions")
    source=db.execute("SELECT sha256 FROM source WHERE source_key=?",(actual[0]["source_key"],)).fetchone()
    if source is None or not re.fullmatch(r"[0-9a-f]{64}",source[0]):
        raise ValueError("Unproven crosswalk source")
    if link.get("source_key")!=actual[0]["source_key"]:
        raise ValueError("Crosswalk provenance changed")
    validated=[]
    seen=set()
    for o in occurrences:
        gid=o.get("occurrence_id")
        side=o.get("side")
        rawhash=o.get("source_blake3")
        if not isinstance(gid,str) or not GAME.fullmatch(gid) or side not in ("white","black"):
            raise ValueError("Invalid official game identity/side")
        if not isinstance(rawhash,str) or not HEX64.fullmatch(rawhash):
            raise ValueError("Invalid source BLAKE3")
        nums=(o.get("source_bytes"),o.get("source_start"),o.get("source_end"),o.get("ordinal"))
        if any(type(n) is not int for n in nums):
            raise ValueError("Invalid source range")
        byt,start,end,ordinal=nums
        if not (0<=start<end<=byt and ordinal>=0):
            raise ValueError("Invalid source range bounds")
        if (gid,side) in seen:
            raise ValueError("Duplicated game occurrence and side")
        seen.add((gid,side))
        clazz=o.get("evidence_class","direct_fide_id")
        if clazz not in ("direct_fide_id","verified_alias","official_context"):
            raise ValueError("Unsupported occurrence evidence class")
        validated.append((coll,gid,side,fide,fsi,rawhash,byt,start,end,ordinal,
                          overlay["core_profile"],actual[0]["source_key"],source[0],clazz))
    # Before beginning any data writes, build the entire proposed ledger.
    db.executescript(DDL)
    inserted=0
    db.execute("BEGIN IMMEDIATE")
    try:
        for item in validated:
            existing=db.execute("""SELECT * FROM verified_core_player_occurrence
                WHERE collection_blake3=? AND occurrence_id=? AND player_side=?""",
                item[:3]).fetchone()
            if existing:
                if tuple(existing)!=item:
                    raise ValueError("Contradictory attribution for existing occurrence")
                continue
            db.execute("""INSERT INTO verified_core_player_occurrence
                VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)""",item)
            inserted+=1
        db.commit()
    except:
        db.rollback()
        raise
    return {"collection_blake3":coll,"requested":len(validated),"inserted":inserted,
            "already_present":len(validated)-inserted,"fsi_id":fsi,"fide_id":fide,
            "crosswalk_source_sha256":source[0],
            "note":"Game identity and dedup stay owned by Antiskill Nucleo; only corroborated FSI/FIDE crosswalks persisted."}

if __name__=="__main__":
    p=argparse.ArgumentParser()
    p.add_argument("--db",required=True,type=Path)
    p.add_argument("--overlay",required=True,type=Path)
    p.add_argument("--report",required=True,type=Path)
    args=p.parse_args()
    data=json.loads(args.overlay.read_text())
    with sqlite3.connect(args.db) as db:
        result=materialize(db,data)
    args.report.write_text(json.dumps(result,ensure_ascii=False,indent=2)+"\n")
    print(json.dumps(result,ensure_ascii=False,indent=2))
