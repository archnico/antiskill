#!/usr/bin/env python3
"""Import local Fide-Scout account *candidates* into a private identity dossier.

No online account becomes an identity automatically, even when the upstream
labels it HIGH or VERIFIED. Linking requires first-party proof and review.
No network access and no new FIDE ID inferred from a username or public name.
"""
import argparse
import collections
import hashlib
import json
import sqlite3
from pathlib import Path
from ingest import NOW

SCHEMA="""
CREATE TABLE IF NOT EXISTS online_account_candidate (
  source_account_id INTEGER NOT NULL,
  fide_id INTEGER NOT NULL,
  platform TEXT NOT NULL,
  account_id TEXT NOT NULL,
  original_status TEXT NOT NULL,
  link_status TEXT NOT NULL CHECK(link_status IN
    ('QUARANTINE','REJECTED_UPSTREAM','IN_SCOPE_UNCONFIRMED')),
  source_key TEXT NOT NULL,
  source_sha256 TEXT NOT NULL,
  evidence_for INTEGER NOT NULL,
  evidence_against INTEGER NOT NULL,
  evidence_neutral INTEGER NOT NULL,
  PRIMARY KEY(source_key,source_account_id)
);
CREATE TABLE IF NOT EXISTS online_account_proof (
  source_key TEXT NOT NULL,
  source_account_id INTEGER NOT NULL,
  source_evidence_id INTEGER NOT NULL,
  direction TEXT NOT NULL,
  evidence_type TEXT NOT NULL,
  strength TEXT NOT NULL,
  source_url TEXT,
  source_fetch_id INTEGER,
  PRIMARY KEY(source_key,source_account_id,source_evidence_id),
  FOREIGN KEY(source_key,source_account_id)
    REFERENCES online_account_candidate(source_key,source_account_id)
);
CREATE INDEX IF NOT EXISTS ix_account_fide ON online_account_candidate(fide_id);
"""

def import_candidates(target_path:Path, source_path:Path, report_path:Path):
    if target_path.resolve()==source_path.resolve():
        raise ValueError("Source and target cannot be the same DB")
    # Source DB is opened read-only; copy an atomic SQLite snapshot in memory.
    # Do not mutate or vacuum third-party databases.
    upstream=sqlite3.connect("file:"+str(source_path.resolve())+"?mode=ro",uri=True)
    snapshot=sqlite3.connect(":memory:")
    upstream.backup(snapshot)
    upstream.close()
    hashes=hashlib.sha256()
    dump_bytes=0
    for line in snapshot.iterdump():
        encoded=line.encode("utf-8","surrogatepass")+b"\n"
        hashes.update(encoded)
        dump_bytes+=len(encoded)
    fingerprint=hashes.hexdigest()
    key="fide-scout-local-"+fingerprint[:16]
    target=sqlite3.connect(target_path)
    target.execute("PRAGMA busy_timeout=5000")
    try:
        target.executescript(SCHEMA)
        target.execute("BEGIN IMMEDIATE")
        target.execute("INSERT OR IGNORE INTO source VALUES (?,?,?,?,?,?)",
                (key,"local private Fide-Scout canonical SQL snapshot",fingerprint,
                 dump_bytes,NOW,"private account candidates, hash of canonical SQL dump; not file bytes"))
        # Correct metadata from an earlier pilot, without replacing evidence IDs.
        target.execute("""UPDATE source SET bytes=?, url=?,
          source_role=? WHERE source_key=? AND sha256=?""",
          (dump_bytes,"local private Fide-Scout canonical SQL snapshot",
           "private account candidates, hash of canonical SQL dump; not file bytes",
           key,fingerprint))
        has_fide={r[0] for r in target.execute("SELECT fide_id FROM fide_player")}
        statuses=collections.Counter()
        account_count=0
        proof_count=0
        rows=snapshot.execute("""
          SELECT a.id,p.fide_id,a.platform,a.username,a.status
          FROM account a JOIN player p ON p.id=a.player_id ORDER BY a.id
        """).fetchall()
        for source_id,fide_raw,platform,username,source_status in rows:
            if not str(fide_raw).isdigit() or int(fide_raw)<=0:
                raise ValueError("Invalid source FIDE ID")
            fid=int(fide_raw)
            if not platform.strip() or not username.strip():
                raise ValueError("Invalid upstream account")
            proof=snapshot.execute("""
              SELECT id,direction,evidence_type,strength,url,fetch_id
              FROM account_evidence WHERE account_id=? ORDER BY id
            """,(source_id,)).fetchall()
            directions=collections.Counter(r[1] for r in proof)
            if source_status=="REJECTED":
                status="REJECTED_UPSTREAM"
            elif fid in has_fide:
                status="IN_SCOPE_UNCONFIRMED"
            else:
                status="QUARANTINE"
            target.execute("INSERT OR IGNORE INTO online_account_candidate VALUES (?,?,?,?,?,?,?,?,?,?,?)",
                (source_id,fid,platform.strip().lower(),username.strip(),source_status,status,key,
                 fingerprint,directions["for"],directions["against"],directions["neutral"]))
            for eid,direction,kind,strength,url,fetch_id in proof:
                target.execute("INSERT OR IGNORE INTO online_account_proof VALUES (?,?,?,?,?,?,?,?)",
                  (key,source_id,eid,direction,kind,strength,url,fetch_id))
                proof_count+=1
            statuses[status]+=1
            account_count+=1
        target.commit()
        if target.execute("PRAGMA integrity_check").fetchone()[0]!="ok":
            raise ValueError("Target SQLite integrity failed")
        report={"source_key":key,"source_fingerprint":fingerprint,
                "accounts_observed":account_count,"evidence_observations":proof_count,
                "statuses":dict(statuses),
                "verified_online_accounts_created":0,
                "note":"All accounts remain candidates; no username/name merge or identity proof granted."}
        report_path.write_text(json.dumps(report,indent=2,ensure_ascii=False)+"\n")
        return report
    except:
        target.rollback()
        raise
    finally:
        target.close()
        snapshot.close()

if __name__=="__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("--db",type=Path,required=True)
    parser.add_argument("--fide-scout-db",type=Path,required=True)
    parser.add_argument("--report",type=Path,required=True)
    args=parser.parse_args()
    print(json.dumps(import_candidates(args.db,args.fide_scout_db,args.report),indent=2,ensure_ascii=False))
