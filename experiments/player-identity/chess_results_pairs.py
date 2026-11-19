#!/usr/bin/env python3
"""Offline source-checked Chess-Results FSI/FIDE crosswalk observations.

Chess-Results/Swiss-Manager is a distinct *publisher* from ALLIN; upstream
registration data can still overlap. Source equality is not independent
verification by itself. This importer only records assertions, never merges.
"""
from __future__ import annotations
import argparse
import collections
import hashlib
import json
import re
import sqlite3
from html.parser import HTMLParser
from pathlib import Path
from ingest import NOW, canonical_name

DDL = """
CREATE TABLE IF NOT EXISTS external_id_pair_observation (
 source_key TEXT NOT NULL,
 tournament_id INTEGER NOT NULL,
 fsi_id INTEGER NOT NULL,
 fide_id INTEGER NOT NULL,
 observed_name TEXT NOT NULL,
 federation TEXT NOT NULL,
 pair_status TEXT NOT NULL CHECK (pair_status IN (
  'AGREES_VERIFIED','AGREES_DISPUTED','CONFLICT_ID',
  'MATCHES_CANDIDATE','UNLINKED_ID_PAIR')),
 source_url TEXT NOT NULL,
 source_sha256 TEXT NOT NULL,
 observed_at TEXT NOT NULL,
 PRIMARY KEY(source_key,fsi_id,fide_id)
);
CREATE INDEX IF NOT EXISTS ix_external_pair_fsi
 ON external_id_pair_observation(fsi_id);
CREATE INDEX IF NOT EXISTS ix_external_pair_fide
 ON external_id_pair_observation(fide_id);
"""

class TableReader(HTMLParser):
    def __init__(self):
        super().__init__()
        self.rows=[]
        self.row=None
        self.cell=None
    def handle_starttag(self,tag,attrs):
        if tag=="tr":
            self.row=[]
        elif tag in ("td","th") and self.row is not None:
            self.cell=[]
    def handle_data(self,text):
        if self.cell is not None:
            self.cell.append(text)
    def handle_endtag(self,tag):
        if tag in ("td","th") and self.cell is not None and self.row is not None:
            self.row.append(" ".join("".join(self.cell).split()))
            self.cell=None
        elif tag=="tr" and self.row is not None:
            self.rows.append(self.row)
            self.row=None

def normalize_signature(name):
    return " ".join(sorted(canonical_name(name).split()))

def parse_tournament(raw):
    reader=TableReader()
    reader.feed(raw.decode("utf-8","replace"))
    groups=[]
    columns=None
    for row in reader.rows:
        if "ID" in row and "ID FIDE" in row and "FED" in row:
            columns=row
            continue
        if columns is None or len(row)!=len(columns):
            continue
        record=dict(zip(columns,row))
        national=record.get("ID","").strip()
        fide=record.get("ID FIDE","").strip()
        if not national.isdigit() or not fide.isdigit() or not int(national) or not int(fide):
            continue
        name=record.get("Nom",record.get("Nome",record.get("Name",""))).strip()
        fed=record["FED"].strip()
        if not fed or len(normalize_signature(name).split())<2:
            raise ValueError("Missing nationality or name in paired-ID row")
        groups.append((int(national),int(fide),name,fed))
    if len(groups)<5:
        raise ValueError("Too few explicit national/FIDE-ID pairs to validate national ID semantics")
    ids=collections.defaultdict(set)
    counterpart=collections.defaultdict(set)
    for a,b,_,_ in groups:
        ids[a].add(b);counterpart[b].add(a)
    if any(len(v)>1 for v in ids.values()) or any(len(v)>1 for v in counterpart.values()):
        raise ValueError("Conflicting explicit ID mappings inside tournament")
    if len(set((a,b) for a,b,_,_ in groups))!=len(groups):
        raise ValueError("Repeated tournament participant IDs")
    return groups

def import_tournament(dbpath:Path,tournament_id:int,html_path:Path,report_path:Path):
    if tournament_id<=0 or not re.fullmatch(r"tnr[0-9]+\.html",html_path.name):
        raise ValueError("Expected explicit Chess-Results tournament file")
    if html_path.name!=f"tnr{tournament_id}.html":
        raise ValueError("Tournament file identity mismatch")
    raw=html_path.read_bytes()
    pair_rows=parse_tournament(raw)
    hash_=hashlib.sha256(raw).hexdigest()
    source_key=f"chess-results-{tournament_id}-{hash_[:12]}"
    source_url=f"https://s3.chess-results.com/tnr{tournament_id}.aspx?SNode=S0&lan=20"
    db=sqlite3.connect(dbpath)
    try:
        db.executescript(DDL)
        existing={(int(fsi),int(fide)):status for fsi,fide,status in db.execute(
            "SELECT fsi_id,fide_id,status FROM identity_link")}
        fsi_mappings=collections.defaultdict(set)
        fide_mappings=collections.defaultdict(set)
        for (a,b),status in existing.items():
            fsi_mappings[a].add(b);fide_mappings[b].add(a)
        candidate_pairs=set(db.execute("SELECT fsi_id,fide_id FROM crosswalk_candidate"))
        verified=sum(existing.get((a,b))=="VERIFIED" for a,b,_,_ in pair_rows)
        # Validate national-ID semantics from the actual full official FSI
        # link registry; avoid treating random national IDs as Italian IDs.
        if verified<4 or verified/len(pair_rows)<0.80:
            raise ValueError("National IDs are not validated as FSI by an adequate trusted baseline")
        statuses=collections.Counter()
        observations=[]
        for a,b,name,fed in pair_rows:
            if (a,b) in existing:
                status="AGREES_VERIFIED" if existing[(a,b)]=="VERIFIED" else "AGREES_DISPUTED"
            elif (fsi_mappings[a] and b not in fsi_mappings[a]) or (
                fide_mappings[b] and a not in fide_mappings[b]):
                status="CONFLICT_ID"
            elif (a,b) in candidate_pairs:
                status="MATCHES_CANDIDATE"
            else:
                status="UNLINKED_ID_PAIR"
            observations.append((source_key,tournament_id,a,b,name,fed,status,
                                 source_url,hash_,NOW))
            statuses[status]+=1
        db.execute("BEGIN IMMEDIATE")
        db.execute("INSERT OR IGNORE INTO source VALUES (?,?,?,?,?,?)",
            (source_key,source_url,hash_,len(raw),NOW,
             "Published tournament FSI+FIDE ID assertion, private use; upstream dependency unverified"))
        db.executemany(
           "INSERT OR IGNORE INTO external_id_pair_observation VALUES (?,?,?,?,?,?,?,?,?,?)",
           observations)
        db.commit()
        if db.execute("PRAGMA integrity_check").fetchone()[0]!="ok":
            raise RuntimeError("Integrity check failed")
        out={"tournament_id":tournament_id,"sha256":hash_,
             "source_key":source_key,"source_url":source_url,
             "explicit_pairs":len(pair_rows),"baseline_verified":verified,
             "baseline_ratio":round(verified/len(pair_rows),4),
             "classifications":dict(statuses),
             "identity_links_created":0,
             "note":"Independent publisher, not necessarily independent underlying registry. Assertions are evidence only, never automatic person merges."}
        report_path.write_text(json.dumps(out,indent=2,ensure_ascii=False)+"\n")
        return out
    finally:
        db.close()

if __name__=="__main__":
    p=argparse.ArgumentParser()
    p.add_argument("--db",type=Path,required=True)
    p.add_argument("--tournament-id",type=int,required=True)
    p.add_argument("--html",type=Path,required=True)
    p.add_argument("--report",type=Path,required=True)
    a=p.parse_args()
    print(json.dumps(import_tournament(a.db,a.tournament_id,a.html,a.report),
                     indent=2,ensure_ascii=False))
