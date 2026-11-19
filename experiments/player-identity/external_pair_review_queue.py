#!/usr/bin/env python3
"""Reconcile external explicit FSI/FIDE IDs against *current* crosswalk state.

An observation's historical import-time pair_status is immutable. Current
review status is recomputed from identity_link on every run to avoid showing
certified players as unresolved. This is a strictly read-only private report.
"""
import argparse
import collections
import csv
import json
import sqlite3
from pathlib import Path

COLUMNS=(
 "priority","fsi_id","fide_id","current_status",
 "observed_initial_statuses","tournaments","source_urls",
 "fsi_official_record_present","fide_official_record_present",
 "crosswalk_source_keys"
)

def safe_cell(value):
    s=str(value) if value is not None else ""
    return "'"+s if s and s[0] in "=+-@\t\r" else s

def review(db,out_csv:Path,out_json:Path):
    groups=collections.defaultdict(list)
    for r in db.execute("""
        SELECT source_key,tournament_id,fsi_id,fide_id,pair_status,source_url,source_sha256
        FROM external_id_pair_observation ORDER BY tournament_id,source_key
    """):
        groups[(int(r["fsi_id"]),int(r["fide_id"]))].append(dict(r))
    # Source hashes are checked independently of the original classifier.
    for records in groups.values():
        for row in records:
            source=db.execute("SELECT sha256 FROM source WHERE source_key=?",
                              (row["source_key"],)).fetchone()
            if source is None or source["sha256"]!=row["source_sha256"]:
                raise ValueError("External source fingerprint is inconsistent")
    statuses=collections.Counter()
    queue=[]
    for (fsi,fide),observations in groups.items():
        links=[dict(r) for r in db.execute("""
            SELECT fsi_id,fide_id,status,source_key FROM identity_link
            WHERE fsi_id=? OR fide_id=? ORDER BY fsi_id,fide_id
        """,(fsi,fide))]
        matching=[r for r in links if (r["fsi_id"],r["fide_id"])==(fsi,fide)]
        competing=[r for r in links if (r["fsi_id"],r["fide_id"])!=(fsi,fide)]
        if competing or len(matching)>1:
            status="CONFLICT_REVIEW";rank=0
        elif matching:
            if matching[0]["status"]=="VERIFIED":
                status="VERIFIED_NOW";rank=3
            else:
                status="DISPUTED_EXISTING_LINK";rank=1
        elif db.execute("""SELECT 1 FROM crosswalk_candidate
                        WHERE fsi_id=? AND fide_id=? LIMIT 1""",
                        (fsi,fide)).fetchone():
            status="CANDIDATE_WITH_EXPLICIT_IDS";rank=2
        else:
            status="UNLINKED_WITH_EXPLICIT_IDS";rank=2
        statuses[status]+=1
        fide_official=bool(db.execute(
            "SELECT 1 FROM fide_player WHERE fide_id=? AND source_key='fide-current'",
            (fide,)).fetchone())
        fsi_official=bool(db.execute(
            "SELECT 1 FROM fsi_official_name_observation WHERE fsi_id=?",
            (fsi,)).fetchone()) or bool(db.execute(
            "SELECT 1 FROM fsi_player WHERE fsi_id=? AND source_key LIKE 'fsi-%'",
            (fsi,)).fetchone())
        row=(
          rank,fsi,fide,status,
          ";".join(sorted({o["pair_status"] for o in observations})),
          ";".join(str(x) for x in sorted({o["tournament_id"] for o in observations})),
          ";".join(sorted({o["source_url"] for o in observations})),
          int(fsi_official),int(fide_official),
          ";".join(sorted({r["source_key"] for r in matching}))
        )
        queue.append(row)
    queue.sort(key=lambda row:(row[0],row[1],row[2]))
    with out_csv.open("w",newline="",encoding="utf-8") as f:
        writer=csv.writer(f)
        writer.writerow(COLUMNS)
        writer.writerows([tuple(safe_cell(x) for x in row) for row in queue])
    summary={
        "distinct_explicit_id_pairs":len(queue),
        "external_observations":sum(len(x) for x in groups.values()),
        "current_statuses":dict(statuses),
        "verified_links_created_by_report":0,
        "note":"Historical observation classifications are never rewritten; current status is recalculated. Local-only CSV."
    }
    out_json.write_text(json.dumps(summary,indent=2,ensure_ascii=False)+"\n")
    return summary

if __name__=="__main__":
    p=argparse.ArgumentParser()
    p.add_argument("--db",type=Path,required=True)
    p.add_argument("--out-csv",type=Path,required=True)
    p.add_argument("--summary-json",type=Path,required=True)
    a=p.parse_args()
    with sqlite3.connect(f"file:{a.db.resolve()}?mode=ro",uri=True) as con:
        con.row_factory=sqlite3.Row
        print(json.dumps(review(con,a.out_csv,a.summary_json),indent=2,ensure_ascii=False))
