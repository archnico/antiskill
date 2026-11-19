#!/usr/bin/env python3
"""Read-only FSI/FIDE player dossiers from locally imported evidence.

A person's string name is a mention, never a sufficient identity link.
All association and prize matches remain *candidates*, not facts of identity.
"""
import argparse
import json
import sqlite3
from pathlib import Path
from ingest import canonical_name

DEFAULT_DB = Path("identity.sqlite3")

def has_table(db, table):
    return db.execute(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?",(table,)).fetchone() is not None

def get(db,table,where,args):
    return [dict(r) for r in db.execute(
        f"SELECT * FROM {table} WHERE {where}", args)]

def club_name_for_year_subquery(alias, year_expression):
    # A returned name is a dated observation, not necessarily the club's current label.
    return f"""COALESCE((SELECT h.official_name FROM club_name_history h
        WHERE h.club_code={alias}.club_code AND h.observed_year={year_expression}
        ORDER BY h.source_key DESC LIMIT 1), c.official_name) AS official_name"""

def profile(db,namespace,external_id):
    db.row_factory=sqlite3.Row
    if namespace=="fsi":
        players=get(db,"fsi_player","fsi_id=?",(external_id,))
        links=get(db,"identity_link","fsi_id=?",(external_id,))
        fsi_ids=[external_id]
        fide_ids=[r["fide_id"] for r in links]
    else:
        players=get(db,"fide_player","fide_id=?",(external_id,))
        links=get(db,"identity_link","fide_id=?",(external_id,))
        fsi_ids=[r["fsi_id"] for r in links]
        fide_ids=[external_id]
    if len(fsi_ids)>25:
        raise ValueError("Identity conflict: too many linked FSI candidates")
    if not players and not links:
        return {"found":False,"namespace":namespace,"id":external_id}
    has_history=has_table(db,"club_name_history")
    aliases=[]; memberships=[]; offices=[]; legacy_medals=[]; full_honours=[]
    for fsi in fsi_ids:
        aliases.extend(get(db,"alias","namespace=? AND player_id=?",("fsi",fsi)))
        club_column=(club_name_for_year_subquery("m","m.membership_year")
                     if has_history else "c.official_name")
        memberships.extend([dict(r) for r in db.execute(f"""
           SELECT m.membership_year,m.club_code,{club_column},
                  m.evidence_kind,m.source_key
           FROM club_membership m LEFT JOIN club c ON c.club_code=m.club_code
           WHERE m.fsi_id=? ORDER BY m.membership_year DESC""",(fsi,))])
        if has_table(db,"club_office_observation"):
            column=(club_name_for_year_subquery("o","o.observed_year")
                    if has_history else "c.official_name")
            offices.extend([dict(r) for r in db.execute(f"""
              SELECT o.club_code,{column},o.observed_name,o.role_label,
                     o.observed_year,o.verification,o.source_key,o.source_url
              FROM club_office_observation o
              LEFT JOIN club c ON c.club_code=o.club_code
              WHERE o.resolved_fsi_candidate=?
              ORDER BY o.observed_year DESC,o.role_label""",(fsi,))])
        if has_table(db,"award_observation"):
            legacy_medals.extend([dict(r) for r in db.execute("""
              SELECT event_code,event_year,edition,medal_placement,observed_name,
                     venue,event_kind,source_url,verification
              FROM award_observation WHERE candidate_fsi_id=?
              ORDER BY event_year DESC,medal_placement""",(fsi,))])
        if has_table(db,"honour_roll_entry") and has_table(db,"honour_roll_placement"):
            full_honours.extend([dict(r) for r in db.execute("""
              SELECT e.event_name,e.event_year,e.edition,e.classification,
                     e.location,p.placement,p.original_label,p.link_status,
                     e.source_key,e.source_sha256
              FROM honour_roll_placement p
              JOIN honour_roll_entry e ON e.entry_id=p.entry_id
              WHERE p.candidate_fsi_id=?
              ORDER BY e.event_year DESC,p.placement,e.event_name""",(fsi,))])
    for fide in fide_ids:
        aliases.extend(get(db,"alias","namespace=? AND player_id=?",("fide",fide)))
    official=[]
    for fid in fide_ids:
        official.extend(get(db,"fide_player","fide_id=?",(fid,)))
    national=[]
    for fsi in fsi_ids:
        national.extend(get(db,"fsi_player","fsi_id=?",(fsi,)))
    return {
        "found":True,
        "query":{"namespace":namespace,"id":external_id},
        "national_records":national,
        "fide_records":official,
        "identity_links":links,
        "aliases":aliases,
        "membership_history":memberships,
        "association_office_candidates":offices,
        "honour_roll_candidates":full_honours,
        "legacy_sample_award_candidates":legacy_medals,
        "warning":"Only FSI/FIDE crosswalk identities can be VERIFIED. Office and medal candidates are NEVER proof of person identity. Do not treat team wins as individual wins."
    }

def find_names(db,name):
    db.row_factory=sqlite3.Row
    terms=[t for t in name.strip().split() if t]
    if not 1<=len(terms)<=8:
        raise ValueError("Supply 1–8 nonempty name tokens")
    where=" AND ".join("full_name LIKE ? COLLATE NOCASE" for _ in terms)
    rows=db.execute("SELECT fsi_id,full_name,category,latest_member_year "
                    "FROM fsi_player WHERE "+where+" LIMIT 200",
                    tuple("%"+term+"%" for term in terms)).fetchall()
    wanted=set(canonical_name(name).split())
    filtered=[dict(r) for r in rows
              if wanted.issubset(set(canonical_name(r["full_name"]).split()))]
    return {"matches":filtered,"more_may_exist":len(rows)>=200,
            "warning":"Name search returns separate candidate identities; never auto-merge them."}

if __name__=="__main__":
    p=argparse.ArgumentParser()
    g=p.add_mutually_exclusive_group(required=True)
    g.add_argument("--fsi-id",type=int)
    g.add_argument("--fide-id",type=int)
    g.add_argument("--name")
    p.add_argument("--db",type=Path,default=DEFAULT_DB)
    args=p.parse_args()
    with sqlite3.connect(args.db) as db:
        out=find_names(db,args.name) if args.name else (
            profile(db,"fsi",args.fsi_id) if args.fsi_id is not None
            else profile(db,"fide",args.fide_id))
    print(json.dumps(out,ensure_ascii=False,indent=2))
