#!/usr/bin/env python3
"""Private, immutable FSI/FIDE player profile index, sourced from national registry,
official scoped FSI rosters and one locally authorized FIDE combined monthly ZIP.
No network, no public export. Ratings 0 mean unrated (JSON null), not Elo 0.
"""
from __future__ import annotations
import argparse
import csv
import datetime as dt
import hashlib
import io
import json
import os
from pathlib import Path
import sqlite3
import tempfile
import xml.etree.ElementTree as ET
import zipfile

from stats_maintenance import (
    ROOT_DEFAULT, REGISTRY_DEFAULT, identity_export, latest_fide,
    snapshot, sha256, verify_digest, atomic_json
)

SCHEMA="antiskill.stats.player-profiles.v1"
INDEX_DIR="durable/identity/profile-indexes"
POINTER="durable/identity/profiles-current.json"


def source_registry(registry_dir, source_hash):
    for p in sorted(registry_dir.glob("identity_registry_????-??-??.sqlite3"), reverse=True):
        if sha256(p)==source_hash:
            return p
    raise ValueError("Registry snapshot corresponding to VERIFIED identity export absent")


def official_fsi(root, year):
    people={}
    sources=[]
    for key in (f"fsi-members-region-tos-{year}",f"fsi-members-club-17029-{year}"):
        obj=snapshot(root,key)
        if obj is None:continue
        verify_digest(obj)
        sources.append({"key":key,"blake3":obj["digest"]})
        raw=Path(obj["path"]).read_bytes()
        try:decoded=raw.decode("utf-8")
        except UnicodeDecodeError:decoded=raw.decode("latin-1")
        for row in csv.reader(io.StringIO(decoded),delimiter=";"):
            if len(row)!=15:raise ValueError(f"FSI {key}: CSV invalid layout")
            who=row[0].strip()
            if not who.isdigit():raise ValueError(f"FSI {key}: nonnumeric id")
            new={
                "surname":row[1].strip(),
                "given_name":row[2].strip(),
                "birth_year":int(row[3]) if row[3].isdigit() and int(row[3])>1850 else None,
                "fsi_category":row[4].strip() or None,
                "nationality_code":row[7].strip() or None,
                "club":row[8].strip() or None,
                "fsi_name_source":key
            }
            if who in people:
                old=people[who]
                for field in ("surname","given_name","birth_year","fsi_category","nationality_code"):
                    if old[field]!=new[field]:
                        raise ValueError(f"FSI duplicate ID {who} has divergent {field}")
            else:people[who]=new
    return people,sources


def load_registry(db_path, crosswalk):
    selected={}
    with open(crosswalk,encoding="utf-8",newline="") as f:
        reader=csv.DictReader(f)
        if reader.fieldnames!=["fsi_id","fide_id","status"]:
            raise ValueError("VERIFIED crosswalk CSV header changed")
        for row in reader:
            if row["status"]=="VERIFIED":
                if row["fsi_id"] in selected:
                    raise ValueError("Duplicate verified FSI id in crosswalk")
                selected[row["fsi_id"]]=row["fide_id"]
    if not selected:raise ValueError("No verified identifiers")
    con=sqlite3.connect(db_path.as_uri()+"?mode=ro",uri=True)
    try:
        rows=con.execute("""
        SELECT i.fsi_id,i.fide_id,i.status,
               f.full_name,f.birth_year,f.category,f.federation,f.source_key,
               g.full_name,g.birth_year,g.fed,g.title,g.source_key
        FROM identity_link i
        LEFT JOIN fsi_player f ON f.fsi_id=i.fsi_id
        LEFT JOIN fide_player g ON g.fide_id=i.fide_id
        WHERE i.status='VERIFIED'
        """).fetchall()
    finally:con.close()
    if len(rows)!=len(selected):raise ValueError("Registry VERIFIED count differs from crosswalk")
    players={}
    for row in rows:
        (fsi,fide,status,fn,fy,fc,ff,fs,gn,gy,gfed,gt,gs)=row
        fsi=str(fsi);fide=str(fide)
        if selected.get(fsi)!=fide:
            raise ValueError("Crosswalk/registry pair changed or conflicts")
        players[fsi]={
            "fsi_id":fsi,"fide_id":fide,
            "full_name":fn or gn,
            "surname":None,"given_name":None,
            "birth_year":fy or gy,
            "birth_date":None,
            "nationality_code":None,
            "fide_federation":gfed or None,
            "fsi_category":fc or None,
            "fsi_category_source":f"registry:{fs}" if fc else None,
            "fide_title":gt or None,
            "fide_standard":None,"fide_rapid":None,"fide_blitz":None,
            "fide_rating_status":"not_in_monthly_list",
            "name_source":f"registry:{fs}" if fn else f"registry:{gs}",
            "fsi_official_as_of":None
        }
    return players


def apply_fsi(players,official,year):
    for fsi, data in official.items():
        if fsi not in players:continue
        p=players[fsi]
        if (p["birth_year"] and data["birth_year"]
            and p["birth_year"]!=data["birth_year"]):
            p["birth_year_conflict"]={
                "registry_year":p["birth_year"],"fsi_year":data["birth_year"]
            }
        p["full_name"]=data["surname"]+" "+data["given_name"]
        p["surname"]=data["surname"]
        p["given_name"]=data["given_name"]
        p["name_source"]=data["fsi_name_source"]
        p["birth_year"]=data["birth_year"] or p["birth_year"]
        p["nationality_code"]=data["nationality_code"]
        p["fsi_category"]=data["fsi_category"]
        p["fsi_category_source"]=data["fsi_name_source"]
        p["fsi_official_as_of"]=str(year)
        p["fsi_club"]=data["club"]


def fide_rating(value):
    if value is None or not value.strip():return None
    if not value.strip().isdigit():raise ValueError("Invalid FIDE rating")
    n=int(value.strip())
    if n==0:return None
    if not 100<=n<=4000:raise ValueError("FIDE rating outside expected limits")
    return n


def scan_fide(source, players, min_seen=1_000_000):
    by_fide={p["fide_id"]:p for p in players.values()}
    z=zipfile.ZipFile(source)
    try:
        names=z.namelist()
        if len(names)!=1 or not names[0].lower().endswith(".xml") or names[0].startswith("/") or ".." in names[0]:
            raise ValueError("FIDE ZIP must have exactly one safe XML member")
        matched=0
        seen=0
        with z.open(names[0]) as stream:
            context=ET.iterparse(stream,events=("start","end"))
            event, root=next(context)
            if event!="start" or root.tag!="playerslist":
                raise ValueError("Unsupported FIDE XML root")
            for event,elem in context:
                if event!="end" or elem.tag!="player":continue
                seen+=1
                fide=elem.findtext("fideid","").strip()
                player=by_fide.get(fide)
                if player is not None:
                    if player["fide_rating_status"]=="found":
                        raise ValueError(f"Duplicate FIDE record {fide}")
                    country=elem.findtext("country","").strip()
                    name=elem.findtext("name","").strip()
                    year=elem.findtext("birthday","").strip()
                    born=int(year) if year.isdigit() and 1850<=int(year)<=2100 else None
                    if (born and player["birth_year"] and player["birth_year"]!=born):
                        player["fide_rating_status"]="birth_year_conflict"
                        player["fide_birth_year"]=born
                    else:
                        player["fide_rating_status"]="found"
                        player["birth_year"]=player["birth_year"] or born
                        player["fide_standard"]=fide_rating(elem.findtext("rating"))
                        player["fide_rapid"]=fide_rating(elem.findtext("rapid_rating"))
                        player["fide_blitz"]=fide_rating(elem.findtext("blitz_rating"))
                    player["fide_federation"]=country or player["fide_federation"]
                    player["fide_title"]=(elem.findtext("title","").strip()
                                           or elem.findtext("w_title","").strip()
                                           or player["fide_title"])
                    if player["surname"] is None and name and "," in name:
                        surname,given=name.split(",",1)
                        player["surname"]=surname.strip() or None
                        player["given_name"]=given.strip() or None
                    if not player["full_name"] and name:
                        player["full_name"]=name
                        player["name_source"]="official-fide-monthly-xml"
                    matched+=1
                elem.clear()
                root.clear()
        if seen<min_seen:
            raise ValueError(f"FIDE XML too short: {seen} rows")
        return {"xml_players_seen":seen,"linked_fide_players_found":matched}
    finally:z.close()


def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("--root",type=Path,default=ROOT_DEFAULT)
    ap.add_argument("--registry-dir",type=Path,default=REGISTRY_DEFAULT)
    ap.add_argument("--month",help="default newest locally imported combined FIDE list")
    args=ap.parse_args()
    root=args.root.resolve()
    crosswalk=identity_export(root,args.registry_dir)
    proof=json.loads(crosswalk.with_suffix(".csv.provenance.json").read_text())
    month=args.month or latest_fide(root)
    if month is None:raise ValueError("No local FIDE combined release")
    fide=snapshot(root,f"fide-rating-combined-{month}")
    if not fide:raise ValueError("No FIDE combined release for "+month)
    verify_digest(fide)
    db=source_registry(args.registry_dir,proof["source_db_sha256"])
    fsi,fsis=official_fsi(root,int(month[:4]))
    input_fp=hashlib.sha256(json.dumps({
        "schema":SCHEMA,"month":month,
        "crosswalk_sha256":proof["crosswalk_sha256"],
        "registry_sha256":proof["source_db_sha256"],
        "fide_blake3":fide["digest"],
        "fsi_sources":fsis
    },sort_keys=True).encode()).hexdigest()
    outdir=root/INDEX_DIR
    outdir.mkdir(parents=True,exist_ok=True,mode=0o700)
    dest=outdir/f"profiles-{month}-{input_fp[:16]}.json"
    pointer=root/POINTER
    if dest.exists():
        payload=json.loads(dest.read_text())
        if payload["fingerprint"]!=input_fp:raise ValueError("Profile index fingerprint mismatch")
        result="unchanged"
    else:
        players=load_registry(db,crosswalk)
        apply_fsi(players,fsi,int(month[:4]))
        scan=scan_fide(fide["path"],players)
        payload={
            "schema":SCHEMA,"fingerprint":input_fp,"month":month,
            "created_at":dt.datetime.now(dt.timezone.utc).isoformat(),
            "crosswalk_sha256":proof["crosswalk_sha256"],
            "registry_sha256":proof["source_db_sha256"],
            "fide_blake3":fide["digest"],
            "fsi_sources":fsis,"scan":scan,
            "players":players,
            "publication_allowed":False
        }
        if len(players)!=proof["status_counts"]["VERIFIED"]:
            raise ValueError("Player index does not cover verified identities")
        os.umask(0o077)
        with tempfile.NamedTemporaryFile("w",encoding="utf8",dir=outdir,prefix=".profile-",delete=False) as f:
            tmp=Path(f.name)
            json.dump(payload,f,ensure_ascii=False,separators=(",",":"))
            f.write("\n")
            f.flush()
            os.fsync(f.fileno())
        os.chmod(tmp,0o600)
        os.replace(tmp,dest)
        result="built"
    digest=sha256(dest)
    atomic_json(pointer,{
        "schema":"antiskill.stats.player-profiles-pointer.v1",
        "month":month,"profile_path":str(dest.relative_to(root)),
        "profile_sha256":digest,"fingerprint":input_fp,
        "crosswalk_sha256":proof["crosswalk_sha256"],
        "registry_sha256":proof["source_db_sha256"],
        "fide_blake3":fide["digest"],
        "publication_allowed":False
    })
    print(json.dumps({"status":result,"month":month,"players":len(payload["players"]),
                      "scan":payload.get("scan"),"file":str(dest),
                      "crosswalk_sha256":proof["crosswalk_sha256"]},ensure_ascii=False))

if __name__=="__main__": main()
