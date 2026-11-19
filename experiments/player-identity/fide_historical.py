"""Import local legacy FIDE fixed-width roster as *historical corroboration only*.
Source chain is not independently verified; do not use as current FIDE ratings.
"""
import sqlite3
from pathlib import Path

def import_fixed(db,path):
    # Import function reuses the caller's source/provenance and integer helpers.
    from ingest import source, valid_int, birth_year
    source(db,'fide-legacy-local',path,'local archive; source snapshot date unverified',
           'historical FIDE roster, not official current validation')
    target={r[0] for r in db.execute('SELECT DISTINCT fide_id FROM fsi_fide_assertion')}
    all_count=0; ita=0; linked=0; stored=0
    with path.open('r',encoding='utf-8',errors='replace') as f:
        header=f.readline()
        if header.find('Name')!=15 or header.find('Fed')!=76 or header.find('B-day')!=152:
            raise ValueError('FIDE fixed-width format changed')
        for line in f:
            all_count+=1
            fid=valid_int(line[:15]); fed=line[76:80].strip().upper()
            if fed=='ITA': ita+=1
            if fid in target: linked+=1
            if fid and (fed=='ITA' or fid in target):
                name=line[15:76].strip()
                if not name: continue
                title=line[84:89].strip()
                by=birth_year(line[152:158])
                db.execute('INSERT OR REPLACE INTO fide_player VALUES (?,?,?,?,?,?)',
                    (fid,name,fed,by,title,'fide-legacy-local'))
                db.execute('INSERT OR IGNORE INTO alias VALUES (?,?,?,?)',
                    ('fide',fid,name,'fide-legacy-local'))
                stored+=1
    return dict(fide_source_rows=all_count,fide_ITA=ita,fide_linked_in_allin=linked,
                fide_legacy_stored=stored,
                warning='local FIDE copy from 2025, not a current verification')
