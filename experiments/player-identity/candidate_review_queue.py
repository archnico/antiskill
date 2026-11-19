#!/usr/bin/env python3
"""Private review queue, ordered by independent FSI roster checks.

The queue proposes no identity merges. Name+birth-year matching, even after
official FSI roster corroboration, never constitutes FIDE-ID evidence.
"""
import argparse
import csv
import json
import sqlite3
from collections import Counter
from pathlib import Path

COLUMNS=("review_priority","fsi_id","fide_id","birth_year",
         "name_signature","official_fsi_name","fsi_roster_url",
         "fsi_roster_sha256","fsi_source_sha256","fide_source_sha256",
         "candidate_status","fsi_roster_status")

def csv_safe(val):
    if val is None:
        return ""
    s=str(val)
    return "'"+s if s and s[0] in "=+-@\t\r" else s

def extract(db,output,summary):
    db.row_factory=sqlite3.Row
    entries=db.execute("""
      SELECT c.candidate_id,c.fsi_id,c.fide_id,c.name_signature,c.birth_year,
        c.fsi_source_sha256,c.fide_source_sha256,c.status,
        MAX(CASE WHEN q.check_status='FSI_ROSTER_CONFLICT' THEN 1 ELSE 0 END) has_conflict,
        MAX(CASE WHEN q.check_status='FSI_ROSTER_CORROBORATED' THEN 1 ELSE 0 END) has_corroboration,
        MAX(CASE WHEN q.check_status='FSI_ROSTER_NOT_LISTED' THEN 1 ELSE 0 END) has_not_listed,
        MAX(CASE WHEN q.check_status='FSI_ROSTER_UNPARSEABLE' THEN 1 ELSE 0 END) has_bad_source
      FROM crosswalk_candidate c
      LEFT JOIN official_fsi_roster_probe q ON q.candidate_id=c.candidate_id
      GROUP BY c.candidate_id
    """).fetchall()
    counts=Counter()
    sorted_rows=[]
    for r in entries:
        if r["has_conflict"]:
            rank=0;status="FSI_ROSTER_CONFLICT"
        elif r["has_corroboration"]:
            rank=1;status="FSI_ROSTER_CORROBORATED"
        elif r["has_bad_source"]:
            rank=2;status="FSI_ROSTER_UNPARSEABLE"
        elif r["has_not_listed"]:
            rank=3;status="FSI_ROSTER_NOT_LISTED"
        else:
            rank=4;status="FSI_ROSTER_NOT_CHECKED"
        counts[status]+=1
        sorted_rows.append((rank,r["fsi_id"],r["fide_id"],r,status))
    sorted_rows.sort(key=lambda x:(x[0],x[1],x[2]))
    with output.open("w",encoding="utf-8",newline="") as f:
        writer=csv.writer(f)
        writer.writerow(COLUMNS)
        for rank,fsi,fide,r,status in sorted_rows:
            source=db.execute("""
              SELECT official_name,source_url,source_sha256
              FROM official_fsi_roster_probe
              WHERE candidate_id=? AND check_status=?
              ORDER BY observed_year DESC,observed_at DESC LIMIT 1
            """,(r["candidate_id"],status)).fetchone()
            row=(rank,fsi,fide,r["birth_year"],r["name_signature"],
                 source["official_name"] if source else None,
                 source["source_url"] if source else None,
                 source["source_sha256"] if source else None,
                 r["fsi_source_sha256"],r["fide_source_sha256"],
                 r["status"],status)
            writer.writerow(tuple(csv_safe(v) for v in row))
    result={"candidates":len(sorted_rows),"priorities":dict(counts),
            "links_promoted":0,
            "note":"Do not attribute an FIDE ID from FSI roster corroboration alone. This CSV is private."}
    summary.write_text(json.dumps(result,ensure_ascii=False,indent=2)+"\n")
    return result

if __name__=="__main__":
    p=argparse.ArgumentParser()
    p.add_argument("--db",required=True,type=Path)
    p.add_argument("--out-csv",required=True,type=Path)
    p.add_argument("--summary-json",required=True,type=Path)
    a=p.parse_args()
    with sqlite3.connect(f"file:{a.db.resolve()}?mode=ro",uri=True) as db:
        print(json.dumps(extract(db,a.out_csv,a.summary_json),ensure_ascii=False,indent=2))
