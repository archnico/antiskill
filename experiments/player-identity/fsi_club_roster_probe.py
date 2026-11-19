#!/usr/bin/env python3
"""Bounded 2026 FSI club-roster snapshots and candidate corroboration.

One official club roster covers many FSI IDs. This proves an official FSI
membership record, NOT a matching FIDE ID. Do not change identity_link.
No bulk scrape: <=5 club pages per invocation and >=1.5s between requests.
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
from fsi_roster_probe import SCHEMA as PROBE_SCHEMA, ROSTER

SCHEMA="""
CREATE TABLE IF NOT EXISTS fsi_official_club_roster(
  fsi_id INTEGER NOT NULL, observed_year INTEGER NOT NULL,
  club_code INTEGER NOT NULL, source_key TEXT NOT NULL,
  source_sha256 TEXT NOT NULL,
  PRIMARY KEY(fsi_id,observed_year,club_code,source_sha256)
);
CREATE TABLE IF NOT EXISTS fsi_official_club_roster_snapshot(
  observed_year INTEGER NOT NULL, club_code INTEGER NOT NULL,
  source_key TEXT NOT NULL, source_sha256 TEXT NOT NULL,
  parsed_members INTEGER NOT NULL,
  PRIMARY KEY(observed_year,club_code,source_sha256)
);
"""

def visible(html_fragment):
    return " ".join(html.unescape(re.sub(
        r"<[^>]+>"," ",html_fragment)).split())

def parse_club_roster(raw,club_code,year):
    doc=raw.decode("utf-8","replace")
    hits=[int(x) for x in re.findall(
        r"Trovati\s*<B>(\d+)</B>\s*giocator",doc,re.I)]
    if not hits or len(set(hits))!=1 or hits[0]<1:
        raise ValueError("Missing or inconsistent official roster count")
    anchor=re.search(r'<div\s+class=["\']table-responsive["\']>',doc,re.I)
    if not anchor:
        raise ValueError("Official roster table missing")
    tail=doc[anchor.end():]
    rows=re.findall(r"(?is)<tr[^>]*>(.*?)</tr>",tail)
    observed={}
    for row in rows:
        cells=[visible(c) for c in re.findall(r"(?is)<td[^>]*>(.*?)</td>",row)]
        if len(cells)<12:
            continue
        if not cells[0].isdigit():
            raise ValueError("Invalid FSI ID in official member row")
        fsi=int(cells[0])
        if fsi<=0 or not cells[5].isdigit() or not cells[3].isdigit() or not cells[10].isdigit():
            raise ValueError("Missing FSI roster fields")
        birth=int(cells[5])
        if not 1850<=birth<=year or int(cells[3])!=year or int(cells[10])!=club_code:
            raise ValueError("Mismatched club or year")
        if fsi in observed or len(canonical_name(cells[1]).split())<2:
            raise ValueError("Duplicate ID or invalid official name")
        observed[fsi]={"name":cells[1],"birth_year":birth}
    if len(observed)!=hits[0]:
        raise ValueError(f"FSI roster count disagrees with parsed members ({hits[0]} vs {len(observed)})")
    return observed

def run(dbpath,raw_dir,report,max_clubs=4,year=2026,delay=1.6):
    if not 1<=max_clubs<=5 or not 2000<=year<=2100 or delay<1.5:
        raise ValueError("Bounded source audit only: 1–5 clubs, delay >=1.5")
    raw_dir.mkdir(parents=True,exist_ok=True)
    db=sqlite3.connect(dbpath)
    db.row_factory=sqlite3.Row
    try:
        db.executescript(PROBE_SCHEMA+SCHEMA)
        clubs=db.execute("""
          SELECT m.club_code,COUNT(DISTINCT x.candidate_id) AS n
          FROM crosswalk_candidate x
          JOIN club_membership m ON m.fsi_id=x.fsi_id AND m.membership_year=?
          JOIN club c ON c.club_code=m.club_code
          WHERE x.status='CANDIDATE_ONLY' AND c.official_name_source IS NOT NULL
            AND NOT EXISTS (
              SELECT 1 FROM fsi_official_club_roster_snapshot s
              WHERE s.club_code=m.club_code AND s.observed_year=?)
          GROUP BY m.club_code ORDER BY n DESC,m.club_code LIMIT ?""",
          (year,year,max_clubs)).fetchall()
        counts=collections.Counter()
        parsed_members=0
        for i,c in enumerate(clubs):
            if i: time.sleep(delay)
            code=c["club_code"]
            url=ROSTER+"?"+urllib.parse.urlencode({"anno":year,"codice":code})
            file=raw_dir/f"fsi-club-roster-{year}-{code}.html"
            if file.exists():
                data=file.read_bytes()
            else:
                request=urllib.request.Request(url,headers={
                    "User-Agent":"AntiskillIdentityPilot/0.1 (bounded club-roster probe)"})
                try:
                    with urllib.request.urlopen(request,timeout=23) as r:
                        if r.status!=200: raise ValueError("Unexpected HTTP response")
                        data=r.read(2_000_001)
                    if not 20_000<=len(data)<=2_000_000:
                        raise ValueError("Unexpected HTML size")
                    file.write_bytes(data)
                except (OSError,ValueError):
                    counts["DOWNLOAD_ERROR"]+=1
                    continue
            try:
                members=parse_club_roster(data,code,year)
            except ValueError:
                counts["ROSTER_FORMAT_ERROR"]+=1
                continue
            digest=hashlib.sha256(data).hexdigest()
            sourcekey=f"fsi-club-roster-{year}-{code}-{digest[:12]}"
            db.execute("BEGIN IMMEDIATE")
            try:
                db.execute("INSERT OR IGNORE INTO source VALUES (?,?,?,?,?,?)",
                  (sourcekey,url,digest,len(data),NOW,"official FSI current club membership, local only"))
                db.execute("INSERT OR IGNORE INTO fsi_official_club_roster_snapshot VALUES (?,?,?,?,?)",
                  (year,code,sourcekey,digest,len(members)))
                for fsi in members:
                    db.execute("INSERT OR IGNORE INTO fsi_official_club_roster VALUES (?,?,?,?,?)",
                      (fsi,year,code,sourcekey,digest))
                rows=db.execute("""
                  SELECT x.candidate_id,x.fsi_id,x.name_signature,x.birth_year
                  FROM crosswalk_candidate x JOIN club_membership m
                  ON x.fsi_id=m.fsi_id AND m.club_code=? AND m.membership_year=?
                  WHERE x.status='CANDIDATE_ONLY'""",(code,year)).fetchall()
                for candidate in rows:
                    person=members.get(candidate["fsi_id"])
                    if person is None:
                        status="FSI_ROSTER_NOT_LISTED"
                        pname=None;birth=None
                    else:
                        pname=person["name"];birth=person["birth_year"]
                        sig=" ".join(sorted(canonical_name(pname).split()))
                        status=("FSI_ROSTER_CORROBORATED"
                          if (sig,birth)==(candidate["name_signature"],candidate["birth_year"])
                          else "FSI_ROSTER_CONFLICT")
                    db.execute("INSERT OR IGNORE INTO official_fsi_roster_probe VALUES (?,?,?,?,?,?,?,?,?)",
                      (candidate["candidate_id"],year,candidate["fsi_id"],pname,birth,status,
                       url,digest,NOW))
                    counts[status]+=1
                db.commit()
            except:
                db.rollback()
                raise
            counts["clubs_parsed"]+=1
            parsed_members+=len(members)
        result={"year":year,"club_pages_requested":len(clubs),
                "official_clubs_parsed":counts["clubs_parsed"],
                "official_memberships_observed":parsed_members,
                "candidate_status_counts":{k:v for k,v in counts.items()
                    if k!="clubs_parsed"},
                "identity_links_promoted":0,
                "note":"The FSI official club roster corroborates FSI identities/memberships only, NOT cross-federation links."}
        report.write_text(json.dumps(result,ensure_ascii=False,indent=2)+"\n")
        return result
    finally:
        db.close()

if __name__=="__main__":
    p=argparse.ArgumentParser()
    p.add_argument("--db",required=True,type=Path)
    p.add_argument("--raw-dir",required=True,type=Path)
    p.add_argument("--report",required=True,type=Path)
    p.add_argument("--max-clubs",type=int,default=4)
    p.add_argument("--year",type=int,default=2026)
    p.add_argument("--delay",type=float,default=1.6)
    a=p.parse_args()
    print(json.dumps(run(a.db,a.raw_dir,a.report,a.max_clubs,a.year,a.delay),
                     ensure_ascii=False,indent=2))
