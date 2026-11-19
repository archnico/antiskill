#!/usr/bin/env python3
"""Historical FSI club directory importer, offline and idempotent.

A club has a stable official code but names, officers and affiliations may change.
Do not overwrite newer club-name display records with older observations.
Name+club+year is only a candidate person identity, never a proven join.
"""
from __future__ import annotations
import collections
import json
import sqlite3
import sys
from pathlib import Path
from enrich_clubs import extract_clubs
from ingest import NOW, canonical_name, sha

BASE="https://www.federscacchi.com/fsi/index.php/struttura/societa"

def apply(db_path: Path, raw_files: list[Path], report: Path):
    if not raw_files:
        raise ValueError("At least one saved FSI society page is required")
    db=sqlite3.connect(db_path)
    db.execute("PRAGMA busy_timeout=10000")
    try:
        db.executescript("""
          CREATE TABLE IF NOT EXISTS club_name_history (
            club_code INTEGER NOT NULL,
            observed_year INTEGER NOT NULL,
            official_name TEXT NOT NULL,
            province TEXT,
            region TEXT,
            source_key TEXT NOT NULL,
            source_sha256 TEXT NOT NULL,
            PRIMARY KEY(club_code,observed_year,source_key)
          );
          CREATE TABLE IF NOT EXISTS club_office_observation (
            club_code INTEGER NOT NULL, observed_name TEXT NOT NULL,
            role_label TEXT NOT NULL, observed_year INTEGER NOT NULL,
            source_key TEXT NOT NULL, source_url TEXT NOT NULL,
            resolved_fsi_candidate INTEGER, verification TEXT NOT NULL,
            PRIMARY KEY(club_code,observed_name,role_label,observed_year,source_key)
          );
        """)
        indexed=collections.defaultdict(set)
        for code, year, fsi, name in db.execute("""
          SELECT m.club_code,m.membership_year,m.fsi_id,p.full_name
          FROM club_membership m JOIN fsi_player p ON p.fsi_id=m.fsi_id
        """):
            indexed[(code,year,canonical_name(name))].add(fsi)
        counts=collections.Counter()
        years=[]
        for f in sorted(raw_files):
            m=__import__("re").search(r"fsi_societa_(20\d\d)\.html$",f.name)
            if not m:
                raise ValueError(f"Cannot establish year from filename: {f}")
            year=int(m.group(1))
            if year<2000 or year>2100:
                raise ValueError("Unexpected year")
            rows=extract_clubs(f)
            years.append(year)
            source_key=f"fsi-societa-{year}-{sha(f)[:12]}"
            url=f"{BASE}?anno={year}&ric=1"
            hash_=sha(f)
            db.execute("INSERT OR IGNORE INTO source VALUES (?,?,?,?,?,?)",
                       (source_key,url,hash_,f.stat().st_size,NOW,
                        f"Official FSI club directory observation for {year}"))
            counts["club_year_observations"]+=len(rows)
            counts["years_scanned"]+=1
            for row in rows:
                code=row["code"]
                db.execute("INSERT OR IGNORE INTO club(club_code) VALUES (?)",(code,))
                db.execute("INSERT OR IGNORE INTO club_name_history VALUES (?,?,?,?,?,?,?)",
                          (code,year,row["name"],row["province"],row["region"],source_key,hash_))
                if year==2026:
                    db.execute("""UPDATE club SET official_name=?,official_name_source=?
                      WHERE club_code=? AND (official_name IS NULL OR official_name_source=?)""",
                       (row["name"],source_key,code,source_key))
                office=[(row["president"],"presidente")]+[(x,"consigliere") for x in row["board"]]
                for name,role in office:
                    c=indexed.get((code,year,canonical_name(name)),set())
                    status=("CANDIDATE" if len(c)==1 else "AMBIGUOUS" if len(c)>1 else "UNLINKED")
                    fsi=next(iter(c)) if len(c)==1 else None
                    db.execute("INSERT OR IGNORE INTO club_office_observation VALUES (?,?,?,?,?,?,?,?)",
                               (code,name,role,year,source_key,url,fsi,status))
                    counts["roles_"+status]+=1
        db.commit()
        if db.execute("PRAGMA integrity_check").fetchone()[0]!="ok":
            raise RuntimeError("Failed SQLite integrity check")
        counts["historical_name_records"]=db.execute("SELECT count(*) FROM club_name_history").fetchone()[0]
        counts["distinct_historical_clubs"]=db.execute("SELECT count(DISTINCT club_code) FROM club_name_history").fetchone()[0]
        counts["office_observations_total"]=db.execute("SELECT count(*) FROM club_office_observation").fetchone()[0]
        counts["club_codes_all"]=db.execute("SELECT count(*) FROM club").fetchone()[0]
        data={"years":years,"counts":dict(counts),
              "warning":"Name+club+year links are candidates only. No confirmed person identity or full elected term is inferred."}
        report.write_text(json.dumps(data,indent=2,ensure_ascii=False)+"\n",encoding="utf-8")
        return data
    except:
        db.rollback()
        raise
    finally:db.close()

if __name__=="__main__":
    if len(sys.argv)<4:
        raise SystemExit("Usage: enrich_club_history.py DB REPORT file1.html [file2.html ...]")
    print(json.dumps(apply(Path(sys.argv[1]),[Path(x) for x in sys.argv[3:]],Path(sys.argv[2])),indent=2,ensure_ascii=False))
