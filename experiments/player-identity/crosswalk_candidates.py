#!/usr/bin/env python3
"""Find still-unlinked Italian FSI/FIDE candidate IDs without creating identity links.

Strict candidate generation: same sorted normalized name tokens, same birth
year, unique on BOTH *full* FSI and FIDE-ITA lists, no prior FSI/FIDE assertion.
These are review leads, never proof of identity or an account attribution.
"""
import argparse
import collections
import re
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
CREATE TABLE IF NOT EXISTS crosswalk_candidate_retirement (
 candidate_id TEXT PRIMARY KEY,
 fsi_id INTEGER NOT NULL,
 fide_id INTEGER NOT NULL,
 original_record_json TEXT NOT NULL,
 retired_at TEXT NOT NULL,
 retirement_reason TEXT NOT NULL
);
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
    # ALLIN assertions are only one provenance class. An independently
    # certified external crosswalk must also suppress name-only candidates.
    existing_fsi={row[0] for row in db.execute(
        "SELECT fsi_id FROM fsi_fide_assertion UNION SELECT fsi_id FROM identity_link")}
    existing_fide={row[0] for row in db.execute(
        "SELECT fide_id FROM fsi_fide_assertion UNION SELECT fide_id FROM identity_link")}
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
        source_fide=sources.get("fide-current")
        if not source_fide:
            raise RuntimeError("Official current FIDE source snapshot missing")
        # Every FSI record owns its own source. New official FSI membership
        # rows MUST NEVER masquerade as originating from ALLIN.
        fsi_sources={fsi:source for fsi,source in
            db.execute("SELECT fsi_id,source_key FROM fsi_player")}
        candidates,ambiguous=calculate(db)
        current_pairs={(sid,fid) for sid,fid,_,_ in candidates}
        previous_pairs={tuple(row) for row in db.execute(
            "SELECT fsi_id,fide_id FROM crosswalk_candidate")}
        obsolete=previous_pairs-current_pairs
        verified_pairs={tuple(row) for row in db.execute(
            "SELECT fsi_id,fide_id FROM identity_link WHERE status='VERIFIED'")}
        ambiguous_retirement=obsolete-verified_pairs
        if ambiguous_retirement:
            raise RuntimeError(
                f"{len(ambiguous_retirement)} candidates invalidated without "
                "a matching verified crosswalk; manual review required")
        inserted=0
        retired=0
        fsi_sources_used=collections.Counter()
        db.execute("BEGIN IMMEDIATE")
        for sid,fid in sorted(obsolete):
            db.row_factory=sqlite3.Row
            old_record=db.execute(
                "SELECT * FROM crosswalk_candidate WHERE fsi_id=? AND fide_id=?",
                (sid,fid)).fetchone()
            if not old_record:
                raise RuntimeError("Previously identified candidate vanished")
            db.execute("INSERT OR IGNORE INTO crosswalk_candidate_retirement VALUES (?,?,?,?,?,?)",
               (old_record["candidate_id"],sid,fid,
                json.dumps(dict(old_record),ensure_ascii=False,sort_keys=True),
                NOW,"PROMOTED_BY_INDEPENDENT_EXPLICIT_VERIFIED_CROSSWALK"))
            db.execute("DELETE FROM crosswalk_candidate WHERE candidate_id=?",
                       (old_record["candidate_id"],))
            retired+=1
        for sid,fid,name,birth_year in candidates:
            source_key=fsi_sources.get(sid)
            source_fsi=sources.get(source_key)
            if not source_fsi or not re.fullmatch(r"[0-9a-f]{64}",source_fsi):
                raise RuntimeError("Missing FSI player-specific source fingerprint")
            claim="|".join(map(str,(sid,fid,source_fsi,source_fide)))
            key=hashlib.sha256(claim.encode("utf-8")).hexdigest()
            before=db.total_changes
            db.execute("""INSERT OR IGNORE INTO crosswalk_candidate
              VALUES (?,?,?,?,?,?,?,?,?,?)""",
              (key,sid,fid,name,birth_year,source_fsi,source_fide,
               "exact_unique_name_tokens_and_birth_year_v1","CANDIDATE_ONLY",NOW))
            if db.total_changes>before:
                inserted+=1
            fsi_sources_used[source_key]+=1
        db.commit()
        if db.execute("PRAGMA integrity_check").fetchone()[0]!="ok":
            raise RuntimeError("Corrupt identity database")
        report={"candidate_pairs":len(candidates),
                "ambiguous_name_year_signatures":ambiguous,
                "new_candidate_pairs_added":inserted,
                "retired_after_explicit_verification":retired,
                "fsi_source_counts":dict(fsi_sources_used),
                "fide_source_sha256":source_fide,
                "source_scope":"FSI people from ALLIN and individual official FSI rosters, FIDE current ITA federation players",
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
