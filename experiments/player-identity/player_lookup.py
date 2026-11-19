#!/usr/bin/env python3
"""Evidence-labelled player lookup for the Antiskill Italia identity pilot.
Read only: never resolves people by name alone and never changes the database.
"""
import argparse
import json
import sqlite3
from pathlib import Path
from ingest import canonical_name
DB=Path('identity.sqlite3')
def get(db,table,where,args):
    db.row_factory=sqlite3.Row
    return [dict(r) for r in db.execute(f'SELECT * FROM {table} WHERE {where}',args)]
def profile(db,namespace,external_id):
    if namespace=='fsi':
        players=get(db,'fsi_player','fsi_id=?',(external_id,))
        links=get(db,'identity_link','fsi_id=?',(external_id,))
        fsi_ids=[external_id]
        fide_ids=[r['fide_id'] for r in links]
    else:
        players=get(db,'fide_player','fide_id=?',(external_id,))
        links=get(db,'identity_link','fide_id=?',(external_id,))
        fsi_ids=[r['fsi_id'] for r in links]
        fide_ids=[external_id]
    ids=tuple(fsi_ids)
    if len(ids)>25: raise ValueError('Ambiguous identity mapping: too many candidates')
    if not players and not links: return {'found':False,'namespace':namespace,'id':external_id}
    aliases=[]; memberships=[]; offices=[]; medals=[]
    for fsi in fsi_ids:
        aliases.extend(get(db,'alias','namespace=? AND player_id=?',('fsi',fsi)))
        memberships.extend([dict(x) for x in db.execute('''
          SELECT m.membership_year, m.club_code,c.official_name,m.evidence_kind,m.source_key
          FROM club_membership m LEFT JOIN club c ON c.club_code=m.club_code
          WHERE m.fsi_id=? ORDER BY m.membership_year DESC''',(fsi,))])
        offices.extend([dict(x) for x in db.execute('''
          SELECT c.club_code,c.official_name, o.observed_name,o.role_label,o.observed_year,
                 o.verification,o.source_url
          FROM club_office_observation o JOIN club c ON c.club_code=o.club_code
          WHERE o.resolved_fsi_candidate=? ORDER BY observed_year DESC''',(fsi,))])
        medals.extend([dict(x) for x in db.execute('''
          SELECT event_code,event_year,edition,medal_placement,observed_name,
                 venue,event_kind,source_url,verification
          FROM award_observation
          WHERE candidate_fsi_id=? ORDER BY event_year DESC, medal_placement''',(fsi,))])
    for fide in fide_ids:
        aliases.extend(get(db,'alias','namespace=? AND player_id=?',('fide',fide)))
    official=[]
    for fid in fide_ids:
        official.extend(get(db,'fide_player','fide_id=?',(fid,)))
    national=[]
    for fsi in fsi_ids:
        national.extend(get(db,'fsi_player','fsi_id=?',(fsi,)))
    return {'found':True,'query':{'namespace':namespace,'id':external_id},
      'national_records':national,'fide_records':official,'identity_links':links,
      'aliases':aliases,'membership_history':memberships,
      'association_office_candidates':offices,'honour_roll_candidates':medals,
      'note':'Office/award matches marked CANDIDATE are not verified identities. Historical FIDE is not current.'}
def find_names(db,name):
    db.row_factory=sqlite3.Row
    terms=[t for t in name.strip().split() if t]
    if not terms or len(terms)>8:
        raise ValueError('Supply 1–8 nonempty name tokens')
    where=' AND '.join('full_name LIKE ? COLLATE NOCASE' for _ in terms)
    rows=db.execute('SELECT fsi_id,full_name,category,latest_member_year '
                    +'FROM fsi_player WHERE '+where+' LIMIT 50',
                    tuple('%'+t+'%' for t in terms)).fetchall()
    wanted=set(canonical_name(name).split())
    exact_token_matches=[dict(r) for r in rows
                         if wanted.issubset(set(canonical_name(r['full_name']).split()))]
    return {'matches':exact_token_matches,
            'warning':'Name search returns separate candidate identities; it never merges them.'}
if __name__=='__main__':
    p=argparse.ArgumentParser()
    g=p.add_mutually_exclusive_group(required=True)
    g.add_argument('--fsi-id',type=int)
    g.add_argument('--fide-id',type=int)
    g.add_argument('--name')
    p.add_argument('--db',type=Path,default=DB)
    args=p.parse_args()
    with sqlite3.connect(args.db) as db:
        out=find_names(db,args.name) if args.name else (
            profile(db,'fsi',args.fsi_id) if args.fsi_id is not None else profile(db,'fide',args.fide_id))
    print(json.dumps(out,ensure_ascii=False,indent=2))
