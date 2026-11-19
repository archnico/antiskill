#!/usr/bin/env python3
"""Export a local, non-public manual review queue for identity assertions.

This is *not* a fuzzy identity resolver. Neither this export nor its hints
approve or merge any FSI/FIDE identity links.
"""
import argparse
import collections
import csv
import json
import sqlite3
from pathlib import Path
from ingest import canonical_name

COLUMNS = ("fsi_id","fide_id","link_status","reason","triage_hint",
           "fsi_name","fide_name","fsi_birth_year","fide_birth_year",
           "fsi_source","fide_source")

def triage(reason, fsi_name, fide_name):
    if reason=="birth_year_disagrees":
        return "CHECK_BIRTH_YEAR_IN_ORIGINALS"
    if reason=="non_bijective_official_crosswalk":
        return "CHECK_MULTIPLE_ID_ASSERTIONS"
    if reason=="fide_latest_record_not_checked":
        return "CHECK_FIDE_ID_SOURCE_AND_SNAPSHOT"
    if reason=="name_spelling_differs":
        a=canonical_name(fsi_name).split()
        b=canonical_name(fide_name).split()
        if a and b and sorted(a)==sorted(b):
            return "SAME_TOKENS_DIFFERENT_ORDER_OR_FORMAT"
        return "CHECK_SPELLING_TRANSLITERATION_OR_ADDITIONS"
    return "MANUAL_REVIEW"

def csv_safe(s):
    if s is None:
        return ""
    x=str(s)
    return "'"+x if x and x[0] in "=+-@\t\r" else x

def build_queue(db, out_csv, summary_json):
    db.row_factory=sqlite3.Row
    rows=db.execute("""
      SELECT l.fsi_id,l.fide_id,l.status,l.reason,
             fs.full_name AS fsi_name, fd.full_name AS fide_name,
             fs.birth_year AS fsi_birth_year,fd.birth_year AS fide_birth_year,
             fs.source_key AS fsi_source,fd.source_key AS fide_source
      FROM identity_link l
      LEFT JOIN fsi_player fs ON fs.fsi_id=l.fsi_id
      LEFT JOIN fide_player fd ON fd.fide_id=l.fide_id
      WHERE l.status IN ('REVIEW','CONFLICT','SOURCE_ASSERTED')
      ORDER BY l.status,l.reason,l.fsi_id,l.fide_id
    """).fetchall()
    counts=collections.Counter()
    by_status=collections.Counter()
    with out_csv.open("w",encoding="utf-8",newline="") as f:
        writer=csv.writer(f)
        writer.writerow(COLUMNS)
        for row in rows:
            hint=triage(row["reason"],row["fsi_name"] or "",row["fide_name"] or "")
            counts[hint]+=1
            by_status[row["status"]]+=1
            writer.writerow([csv_safe(v) for v in (
              row["fsi_id"],row["fide_id"],row["status"],row["reason"],hint,
              row["fsi_name"],row["fide_name"],row["fsi_birth_year"],
              row["fide_birth_year"],row["fsi_source"],row["fide_source"])])
    summary={"total_review_rows":len(rows),
             "by_status":dict(by_status),
             "triage_hints":dict(counts),
             "note":"All rows require manual evidence review; CSV is for private local use, not distribution."}
    summary_json.write_text(json.dumps(summary,indent=2,ensure_ascii=False)+"\n",encoding="utf-8")
    return summary

if __name__=="__main__":
    p=argparse.ArgumentParser()
    p.add_argument("--db",type=Path,required=True)
    p.add_argument("--out-csv",type=Path,required=True)
    p.add_argument("--summary-json",type=Path,required=True)
    a=p.parse_args()
    with sqlite3.connect(a.db) as db:
        result=build_queue(db,a.out_csv,a.summary_json)
    print(json.dumps(result,indent=2,ensure_ascii=False))
