#!/usr/bin/env python3
"""Antiskill Italian chess player identity pilot. Source-backed, non-destructive.
No name-only identity merges. Keeps source snapshots separate from derived views.
Does not ingest dates of birth, street addresses, emails or phone numbers.
"""
import argparse
import collections
import csv
import datetime as dt
import hashlib
import json
import re
import sqlite3
import unicodedata
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

SCHEMA = """
CREATE TABLE source (source_key TEXT PRIMARY KEY, url TEXT NOT NULL, sha256 TEXT NOT NULL,
  bytes INTEGER NOT NULL, retrieved_at TEXT NOT NULL, source_role TEXT NOT NULL);
CREATE TABLE fsi_player (fsi_id INTEGER PRIMARY KEY, full_name TEXT NOT NULL,
  birth_year INTEGER, federation TEXT, category TEXT, latest_member_year INTEGER,
  reported_fide_id INTEGER, source_key TEXT NOT NULL);
CREATE TABLE fide_player (fide_id INTEGER PRIMARY KEY, full_name TEXT NOT NULL,
  fed TEXT NOT NULL, birth_year INTEGER, title TEXT, source_key TEXT NOT NULL);
CREATE TABLE fsi_fide_assertion (fsi_id INTEGER NOT NULL, fide_id INTEGER NOT NULL,
  source_key TEXT NOT NULL, PRIMARY KEY(fsi_id,fide_id,source_key));
CREATE TABLE identity_link (fsi_id INTEGER NOT NULL, fide_id INTEGER NOT NULL,
  status TEXT NOT NULL, reason TEXT NOT NULL, source_key TEXT NOT NULL,
  PRIMARY KEY(fsi_id,fide_id));
CREATE TABLE club (club_code INTEGER PRIMARY KEY, official_name TEXT, official_name_source TEXT);
CREATE TABLE club_membership (fsi_id INTEGER NOT NULL, club_code INTEGER NOT NULL,
  membership_year INTEGER NOT NULL, evidence_kind TEXT NOT NULL, source_key TEXT NOT NULL,
  PRIMARY KEY(fsi_id,club_code,membership_year,evidence_kind));
CREATE TABLE person_role (fsi_id INTEGER, fide_id INTEGER, club_code INTEGER,
  role_name TEXT NOT NULL, start_date TEXT, end_date TEXT,
  source_url TEXT NOT NULL, source_sha256 TEXT NOT NULL, verification TEXT NOT NULL);
CREATE TABLE achievement (fsi_id INTEGER, fide_id INTEGER, event_id TEXT NOT NULL,
  event_name TEXT NOT NULL, event_year INTEGER, placement INTEGER, category TEXT,
  online_platform TEXT, source_url TEXT NOT NULL, source_sha256 TEXT NOT NULL,
  verification TEXT NOT NULL);
CREATE TABLE online_account (fsi_id INTEGER, fide_id INTEGER, platform TEXT NOT NULL,
  account_id TEXT NOT NULL, verification TEXT NOT NULL, proof_url TEXT NOT NULL,
  checked_at TEXT NOT NULL);
CREATE TABLE alias (namespace TEXT NOT NULL, player_id INTEGER NOT NULL,
  name TEXT NOT NULL, source_key TEXT NOT NULL,
  PRIMARY KEY(namespace,player_id,name,source_key));
CREATE TABLE game_attribution (game_id TEXT NOT NULL, namespace TEXT NOT NULL,
  player_id INTEGER NOT NULL, evidence TEXT NOT NULL, verification TEXT NOT NULL,
  PRIMARY KEY(game_id,namespace,player_id,evidence));
CREATE INDEX ix_club_year ON club_membership(club_code,membership_year);
CREATE INDEX ix_achieve_event ON achievement(event_id,placement);
CREATE INDEX ix_fide_fed ON fide_player(fed);
"""
NOW=dt.datetime.now(dt.timezone.utc).isoformat()

def sha(path):
    h=hashlib.sha256()
    with path.open('rb') as f:
        for b in iter(lambda:f.read(1<<20),b''): h.update(b)
    return h.hexdigest()

def valid_int(value):
    value=(value or '').strip()
    if not value.isdigit(): return None
    n=int(value)
    return n if n>0 else None

def birth_year(value):
    n=valid_int(value)
    return n if n is not None and 1850<=n<=dt.date.today().year else None

def canonical_name(s):
    s=unicodedata.normalize('NFKD',s).casefold()
    s=''.join(c for c in s if not unicodedata.combining(c))
    return ' '.join(re.findall(r'[a-z0-9]+',s))

def source(db,key,path,url,role):
    h=sha(path)
    db.execute('INSERT INTO source VALUES (?,?,?,?,?,?)',(key,url,h,path.stat().st_size,NOW,role))
    return h

def allin_import(db,path):
    source(db,'allin-2026-04',path,'https://torneionline.com/dwn/allin.csv','explicit FSI/FIDE crosswalk and reported memberships')
    total=0; retained=0; explicit=0; club_edges=0
    seen=collections.defaultdict(set)
    # File is semicolon-delimited. Source contains many people without an FSI ID.
    with path.open('r',encoding='cp1252',errors='replace',newline='') as f:
        reader=csv.DictReader(f,delimiter=';')
        required={'ID FSI','FIN','Nominativo','Circolo Attuale','Anno Ultimo Tesseramento'}
        if not required.issubset(reader.fieldnames or []):
            raise ValueError('ALLIN columns changed: '+str(reader.fieldnames))
        for row in reader:
            total+=1
            fsi=valid_int(row['ID FSI'])
            if not fsi: continue
            retained+=1
            fide=valid_int(row.get('FIN'))
            name=(row.get('Nominativo') or '').strip()
            if not name: continue
            by=birth_year(row.get('Anno Nascita'))
            y=valid_int(row.get('Anno Ultimo Tesseramento'))
            db.execute('INSERT OR IGNORE INTO fsi_player VALUES (?,?,?,?,?,?,?,?)',
                (fsi,name,by,(row.get('Federazione') or '').strip(),(row.get('Categoria') or '').strip(),
                 y,fide,'allin-2026-04'))
            db.execute('INSERT OR IGNORE INTO alias VALUES (?,?,?,?)',('fsi',fsi,name,'allin-2026-04'))
            if fide:
                explicit+=1
                seen[fsi].add(fide)
                db.execute('INSERT OR IGNORE INTO fsi_fide_assertion VALUES (?,?,?)',(fsi,fide,'allin-2026-04'))
            if y and 1900<=y<=dt.date.today().year:
                for column,offset,kind in (
                    ('Circolo Attuale',0,'reported_latest'),
                    ('Circolo Anno Precedente',1,'reported_previous'),
                    ('Circolo Due Anni Precedenti',2,'reported_two_years_back')):
                    club=valid_int(row.get(column))
                    if club and y-offset>=1900:
                        db.execute('INSERT OR IGNORE INTO club(club_code) VALUES (?)',(club,))
                        db.execute('INSERT OR IGNORE INTO club_membership VALUES (?,?,?,?,?)',
                            (fsi,club,y-offset,kind,'allin-2026-04'))
                        club_edges+=1
    return dict(source_rows=total,fsi_rows=retained,explicit_fide_cells=explicit,club_edges=club_edges)

def fide_import(db,path):
    if path.suffix.lower()=='.txt':
        from fide_historical import import_fixed
        return import_fixed(db,path)
    source(db,'fide-current',path,'https://ratings.fide.com/download/players_list_xml.zip',
           'FIDE player directory (latest endpoint at retrieval)')
    assert zipfile.is_zipfile(path),'FIDE source is not a ZIP'
    targeted=set(r[0] for r in db.execute('SELECT DISTINCT fide_id FROM fsi_fide_assertion'))
    total=0; ita=0; linked=0; other=0
    with zipfile.ZipFile(path) as z:
        xml_files=[x for x in z.namelist() if x.lower().endswith('.xml')]
        if len(xml_files)!=1: raise ValueError('Expected a single FIDE XML: '+str(xml_files))
        with z.open(xml_files[0]) as stream:
            iterator=ET.iterparse(stream,events=('start','end'))
            _,root=next(iterator)
            for event,elem in iterator:
                if event!='end' or elem.tag.split('}')[-1].lower()!='player': continue
                rec={c.tag.split('}')[-1].lower():(c.text or '').strip() for c in elem}
                fid=valid_int(rec.get('fideid') or rec.get('id_number'))
                fed=(rec.get('country') or rec.get('fed') or '').upper()
                total+=1
                if fed=='ITA': ita+=1
                if fid in targeted: linked+=1
                if fid and (fed=='ITA' or fid in targeted):
                    db.execute('INSERT OR REPLACE INTO fide_player VALUES (?,?,?,?,?,?)',
                        (fid,rec.get('name') or '',fed,birth_year(rec.get('birthday')),
                         rec.get('title') or '','fide-current'))
                    if rec.get('name'):
                        db.execute('INSERT OR IGNORE INTO alias VALUES (?,?,?,?)',
                            ('fide',fid,rec['name'],'fide-current'))
                else: other+=1
                elem.clear()
                root.clear()
    return dict(fide_source_rows=total,fide_ITA=ita,fide_linked_in_allin=linked,fide_other_not_stored=other)

def reconcile(db):
    fsi_map=collections.defaultdict(set); fide_map=collections.defaultdict(set)
    for fsi,fide in db.execute('SELECT fsi_id,fide_id FROM fsi_fide_assertion'):
        fsi_map[fsi].add(fide); fide_map[fide].add(fsi)
    counts=collections.Counter()
    for fsi,ids in fsi_map.items():
        for fide in ids:
            if len(ids)>1 or len(fide_map[fide])>1:
                status='CONFLICT'; reason='non_bijective_official_crosswalk'
            else:
                rec=db.execute('SELECT full_name,birth_year FROM fsi_player WHERE fsi_id=?',(fsi,)).fetchone()
                side=db.execute('SELECT full_name,birth_year FROM fide_player WHERE fide_id=?',(fide,)).fetchone()
                if side is None:
                    status='SOURCE_ASSERTED'; reason='fide_latest_record_not_checked'
                elif rec[1] and side[1] and rec[1]!=side[1]:
                    status='REVIEW'; reason='birth_year_disagrees'
                elif rec[0] and side[0] and canonical_name(rec[0])!=canonical_name(side[0]):
                    # Name differences may be entirely legitimate. Preserve the explicit ID link,
                    # but request review; never infer a new ID from names.
                    status='REVIEW'; reason='name_spelling_differs'
                else:
                    status='VERIFIED'; reason='direct_crosswalk_and_fide_snapshot'
            db.execute('INSERT INTO identity_link VALUES (?,?,?,?,?)',
                       (fsi,fide,status,reason,'allin-2026-04'))
            counts[status]+=1
    return dict(counts)

def build(allin,fide,out):
    tmp=out.with_suffix('.sqlite3.building')
    if tmp.exists(): tmp.unlink()
    db=sqlite3.connect(tmp)
    db.executescript(SCHEMA)
    try:
        metrics={'allin':allin_import(db,allin)}
        db.commit()
        if fide and fide.exists(): metrics['fide']=fide_import(db,fide)
        else: metrics['fide']='not_yet_downloaded'
        metrics['links']=reconcile(db)
        if fide and fide.suffix.lower()=='.txt':
            verified=metrics['links'].pop('VERIFIED',0)
            if verified:
                db.execute("UPDATE identity_link SET status='HISTORICAL_MATCH', reason='2025_archive_corrob_only' WHERE status='VERIFIED'")
                metrics['links']['HISTORICAL_MATCH']=verified
        metrics['counts']={key:db.execute('SELECT count(*) FROM '+key).fetchone()[0]
                for key in ('fsi_player','fide_player','identity_link','club','club_membership','person_role','achievement','online_account')}
        db.commit()
        integrity=db.execute('PRAGMA integrity_check').fetchone()[0]
        if integrity!='ok': raise RuntimeError('SQLite integrity '+integrity)
        db.close()
        tmp.replace(out)
        return metrics
    except:
        db.close()
        raise

if __name__=='__main__':
    ap=argparse.ArgumentParser()
    ap.add_argument('--allin',type=Path,required=True)
    ap.add_argument('--fide',type=Path)
    ap.add_argument('--out',type=Path,required=True)
    ap.add_argument('--report',type=Path,required=True)
    args=ap.parse_args()
    args.out.parent.mkdir(parents=True,exist_ok=True)
    metrics=build(args.allin,args.fide,args.out)
    args.report.parent.mkdir(parents=True,exist_ok=True)
    args.report.write_text(json.dumps({'generated_at':NOW,'metrics':metrics,
        'policy':'source-backed, no name-only merge, no raw DOB/address/contact import',
        'allin_sha256':sha(args.allin),
        'fide_sha256':sha(args.fide) if args.fide and args.fide.exists() else None},
        ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(metrics,ensure_ascii=False,indent=2))
