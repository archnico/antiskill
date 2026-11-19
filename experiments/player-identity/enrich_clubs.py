#!/usr/bin/env python3
"""Read one saved official FSI 2026 club directory page, preserve titles as observations.
Identity links by club+name are CANDIDATE only; never auto-merge people.
"""
import datetime as dt
import hashlib
import html
import json
import re
import sqlite3
import sys
from pathlib import Path
from ingest import canonical_name, sha, NOW

def extract_clubs(path):
    text=path.read_text(encoding='utf-8',errors='replace')
    cards=re.findall(r'(?is)<h2>\s*<B>(.*?)</B>\s*</h2>(.*?)(?=<h2>\s*<B>|$)',text)
    def field(body,label):
        m=re.search(r'<B>\s*'+re.escape(label)+r':\s*</B>\s*&nbsp;\s*([^<]+)',body,re.I)
        return html.unescape(m.group(1)).strip() if m else ''
    records=[]
    for raw_name,body in cards:
        code=field(body,'Id FSI')
        if not code.isdigit(): raise ValueError('Club missing numeric ID: '+raw_name)
        name=html.unescape(re.sub(r'<.*?>','',raw_name)).strip()
        president=field(body,'Presidente')
        board=[x.strip() for x in field(body,'Consiglio').split(';') if x.strip()]
        if not name or not president: raise ValueError('Missing club name/president for '+code)
        records.append(dict(code=int(code),name=name,president=president,board=board,
                            province=field(body,'Provincia'),region=field(body,'Regione')))
    if len(records)!=len({x['code'] for x in records}):
        raise ValueError('Duplicate club codes in FSI directory')
    if not records: raise ValueError('No clubs parsed')
    return records

def enrich(db_path,html_path):
    records=extract_clubs(html_path)
    db=sqlite3.connect(db_path)
    db.execute('PRAGMA busy_timeout=3000')
    db.executescript("""
    CREATE TABLE IF NOT EXISTS club_office_observation (
      club_code INTEGER NOT NULL,
      observed_name TEXT NOT NULL,
      role_label TEXT NOT NULL,
      observed_year INTEGER NOT NULL,
      source_key TEXT NOT NULL,
      source_url TEXT NOT NULL,
      resolved_fsi_candidate INTEGER,
      verification TEXT NOT NULL,
      PRIMARY KEY(club_code,observed_name,role_label,observed_year,source_key));
    CREATE INDEX IF NOT EXISTS idx_office_name ON club_office_observation(club_code,observed_year);
    """)
    key='fsi-societa-2026'
    url='https://www.federscacchi.com/fsi/index.php/struttura/societa?anno=2026&ric=1'
    digest=sha(html_path)
    db.execute("INSERT OR IGNORE INTO source VALUES (?,?,?,?,?,?)",
              (key,url,digest,html_path.stat().st_size,NOW,'official FSI club code,name,president,board listing'))
    # Index FSI player IDs by (official club code, name) only within 2026 membership observations.
    candidates={}
    for code,fsi,name in db.execute("""SELECT m.club_code,m.fsi_id,p.full_name
      FROM club_membership m JOIN fsi_player p ON p.fsi_id=m.fsi_id
      WHERE m.membership_year=2026"""):
        candidates.setdefault((code,canonical_name(name)),set()).add(fsi)
    counters=dict(clubs=0,presidents=0,board_member_observations=0,candidate_person_links=0,
                  ambiguous_person_links=0,unmatched_office_names=0)
    try:
        for c in records:
            db.execute('INSERT OR IGNORE INTO club(club_code) VALUES (?)',(c['code'],))
            db.execute("""UPDATE club SET official_name=?, official_name_source=? WHERE club_code=?""",
                       (c['name'],key,c['code']))
            counters['clubs']+=1
            roles=[(c['president'],'presidente')]+[(x,'consigliere') for x in c['board']]
            counters['presidents']+=1
            counters['board_member_observations']+=len(c['board'])
            for person,role in roles:
                cand=candidates.get((c['code'],canonical_name(person)),set())
                status='CANDIDATE' if len(cand)==1 else ('AMBIGUOUS' if len(cand)>1 else 'UNLINKED')
                candidate=next(iter(cand)) if len(cand)==1 else None
                if status=='CANDIDATE': counters['candidate_person_links']+=1
                if status=='AMBIGUOUS': counters['ambiguous_person_links']+=1
                if status=='UNLINKED': counters['unmatched_office_names']+=1
                db.execute("""INSERT OR REPLACE INTO club_office_observation
                  VALUES (?,?,?,?,?,?,?,?)""",(c['code'],person,role,2026,key,url,candidate,status))
        db.commit()
        if db.execute('PRAGMA integrity_check').fetchone()[0]!='ok': raise ValueError('database error')
        counters['clubs_with_allin_memberships']=db.execute("""SELECT count(DISTINCT c.club_code)
          FROM club c JOIN club_membership m USING(club_code)
          WHERE c.official_name_source=?""",(key,)).fetchone()[0]
        counters['total_clubs_after_import']=db.execute('SELECT count(*) FROM club').fetchone()[0]
        counters['office_observations']=db.execute('SELECT count(*) FROM club_office_observation').fetchone()[0]
    finally: db.close()
    return {'source_sha256':digest,'metrics':counters}

if __name__=='__main__':
    if len(sys.argv)!=4: raise SystemExit('usage: enrich_clubs.py DB FSI_HTML REPORT_JSON')
    db_path,html_path,report=map(Path,sys.argv[1:])
    data=enrich(db_path,html_path)
    report.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(data,ensure_ascii=False,indent=2))
