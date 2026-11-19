#!/usr/bin/env python3
"""Bounded read-only verification of FSI candidate details against FSI roster.

IMPORTANT: A corroborated FSI name and birth year is NOT a verified FSI/FIDE
crosswalk. Never update identity_link or crosswalk_candidate.status.
"""
import argparse
import collections
import hashlib
import html
import json
import re
import sqlite3
import time
import urllib.parse
import urllib.request
from pathlib import Path
from ingest import NOW, canonical_name

ROSTER="https://www.federscacchi.com/fsi/index.php/struttura/tesserati"
SCHEMA="""
CREATE TABLE IF NOT EXISTS official_fsi_roster_probe(
 candidate_id TEXT NOT NULL, observed_year INTEGER NOT NULL,
 fsi_id INTEGER NOT NULL, official_name TEXT, official_birth_year INTEGER,
 check_status TEXT NOT NULL CHECK(check_status IN (
   'FSI_ROSTER_CORROBORATED','FSI_ROSTER_CONFLICT',
   'FSI_ROSTER_NOT_LISTED','FSI_ROSTER_UNPARSEABLE')),
 source_url TEXT NOT NULL, source_sha256 TEXT NOT NULL, observed_at TEXT NOT NULL,
 PRIMARY KEY(candidate_id,observed_year,source_sha256)
);
"""

def parse_roster(raw, fsi_id):
    page=raw.decode("utf-8","replace")
    hits=[int(n) for n in re.findall(r"Trovati\s*<B>(\d+)</B>\s*giocator",page,re.I)]
    if not hits or len(set(hits))!=1:
        return ("FSI_ROSTER_UNPARSEABLE",None,None)
    if hits[0]==0:
        return ("FSI_ROSTER_NOT_LISTED",None,None)
    if hits[0]!=1:
        return ("FSI_ROSTER_UNPARSEABLE",None,None)
    pattern=r"<A\s+HREF=[^>]*\bifsi=(\d+)[^>]*>\s*(\d+)\s*</A>"
    matches=[m for m in re.finditer(pattern,page,re.I)
             if int(m.group(1))==fsi_id and int(m.group(2))==fsi_id]
    if len(matches)!=1:
        return ("FSI_ROSTER_UNPARSEABLE",None,None)
    part=page[matches[0].end():matches[0].end()+1600]
    found_name=re.search(
        r"<!--\s*nome giocatore\s*-->.*?<B>\s*(?:&nbsp;|\s)*(.*?)</B>",
        part,re.I|re.S)
    found_year=re.search(r"Nato nel(?:\s|&nbsp;)*<B>\s*(\d{4})\s*</B>",part,re.I|re.S)
    if not found_name or not found_year:
        return ("FSI_ROSTER_UNPARSEABLE",None,None)
    name=html.unescape(re.sub(r"<[^>]*>","",found_name.group(1))).strip()
    birth=int(found_year.group(1))
    if len(canonical_name(name).split())<2 or not 1850<=birth<=2100:
        return ("FSI_ROSTER_UNPARSEABLE",None,None)
    return ("PARSED",name,birth)

def check_candidate(db,candidate,raw,url,year):
    state,name,birth=parse_roster(raw,int(candidate["fsi_id"]))
    if state=="PARSED":
        signature=" ".join(sorted(canonical_name(name).split()))
        state=("FSI_ROSTER_CORROBORATED" if
               (signature,birth)==(candidate["name_signature"],candidate["birth_year"])
               else "FSI_ROSTER_CONFLICT")
    digest=hashlib.sha256(raw).hexdigest()
    db.execute("""INSERT OR IGNORE INTO official_fsi_roster_probe
      VALUES (?,?,?,?,?,?,?,?,?)""",
      (candidate["candidate_id"],year,candidate["fsi_id"],name,birth,
       state,url,digest,NOW))
    return state

def run(dbpath,raw_dir,report,max_checks=8,year=2026,delay=1.3):
    if not 1<=max_checks<=12 or not 2000<=year<=2100 or delay<1.0:
        raise ValueError("Pilot limited to 1–12 requests, min. 1s interval, year 2000–2100")
    raw_dir.mkdir(parents=True,exist_ok=True)
    db=sqlite3.connect(dbpath)
    db.row_factory=sqlite3.Row
    try:
        db.executescript(SCHEMA)
        # Adult-only pilot, selected deterministically. NOT random sampling.
        rows=db.execute("""
          SELECT x.candidate_id,x.fsi_id,x.fide_id,x.name_signature,x.birth_year
          FROM crosswalk_candidate x JOIN fsi_player p USING(fsi_id)
          WHERE p.latest_member_year=? AND x.birth_year<=?
            AND x.status='CANDIDATE_ONLY'
            AND NOT EXISTS (SELECT 1 FROM official_fsi_roster_probe q
              WHERE q.candidate_id=x.candidate_id AND q.observed_year=?)
          ORDER BY x.fsi_id LIMIT ?""",(year,year-18,year,max_checks)).fetchall()
        counts=collections.Counter()
        for index,row in enumerate(rows):
            if index: time.sleep(delay)
            url=ROSTER+"?"+urllib.parse.urlencode(
                {"anno":year,"idx":row["fsi_id"],"ric":1})
            path=raw_dir/f"fsi-roster-{year}-id-{row['fsi_id']}.html"
            if path.exists():
                raw=path.read_bytes()
            else:
                req=urllib.request.Request(url,headers={
                    "User-Agent":"AntiskillIdentityPilot/0.1"})
                try:
                    with urllib.request.urlopen(req,timeout=18) as response:
                        if response.status!=200: raise ValueError("HTTP not 200")
                        raw=response.read(2_000_001)
                    if not 20_000<=len(raw)<=2_000_000:
                        raise ValueError("Unexpected response size")
                    path.write_bytes(raw)
                except (OSError,ValueError) as exc:
                    counts["DOWNLOAD_ERROR"]+=1
                    continue
            status=check_candidate(db,row,raw,url,year)
            counts[status]+=1
            db.commit()
        result={
            "year":year,"attempted":len(rows),"status_counts":dict(counts),
            "roster_evidence_observations":db.execute(
                "SELECT count(*) FROM official_fsi_roster_probe").fetchone()[0],
            "identity_links_promoted":0,
            "note":"FSI name/year corroborated independently; FSI/FIDE ID crosswalk NOT proven"
        }
        report.write_text(json.dumps(result,ensure_ascii=False,indent=2)+"\n")
        return result
    finally:
        db.close()

if __name__=="__main__":
    p=argparse.ArgumentParser()
    p.add_argument("--db",required=True,type=Path)
    p.add_argument("--raw-dir",required=True,type=Path)
    p.add_argument("--report",required=True,type=Path)
    p.add_argument("--max-checks",type=int,default=8)
    p.add_argument("--year",type=int,default=2026)
    p.add_argument("--delay",type=float,default=1.3)
    a=p.parse_args()
    print(json.dumps(run(a.db,a.raw_dir,a.report,a.max_checks,a.year,a.delay),
                     ensure_ascii=False,indent=2))
