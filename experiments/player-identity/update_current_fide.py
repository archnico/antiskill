#!/usr/bin/env python3
"""Integrate independently verified current FIDE snapshot without losing FSI offices/awards.
Creates an SQLite backup before mutation; restores nothing automatically.
"""
import json
import sqlite3
import sys
from pathlib import Path
from ingest import fide_import,reconcile,sha,NOW

def upgrade(db_path,fide_zip,report):
    if not fide_zip.exists() or fide_zip.suffix.lower()!='.zip':
        raise ValueError('Need completed FIDE ZIP, not a partial file')
    backup=db_path.with_name(db_path.stem+'_before_fide_current.sqlite3')
    if backup.exists():
        raise FileExistsError('Safety backup exists: '+str(backup))
    con=sqlite3.connect(db_path)
    try:
        with sqlite3.connect(backup) as bc:
            con.backup(bc)
        con.execute('BEGIN IMMEDIATE')
        con.execute('DELETE FROM fide_player')
        con.execute('DELETE FROM identity_link')
        metrics={'fide_current':fide_import(con,fide_zip)}
        metrics['links']=reconcile(con)
        for k in ('fsi_player','fide_player','identity_link','club',
                  'club_membership','club_office_observation','award_observation'):
            metrics[k]=con.execute('SELECT count(*) FROM '+k).fetchone()[0]
        if con.execute('PRAGMA integrity_check').fetchone()[0]!='ok':
            raise ValueError('SQLite failed integrity check')
        con.commit()
        metrics['zip_sha256']=sha(fide_zip)
        metrics['finished_at']=NOW
        report.write_text(json.dumps(metrics,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
        return metrics
    except:
        con.rollback()
        raise
    finally: con.close()

if __name__=='__main__':
    if len(sys.argv)!=4: raise SystemExit('usage: update_current_fide.py DB FIDE_ZIP REPORT_JSON')
    m=upgrade(*map(Path,sys.argv[1:]))
    print(json.dumps(m,ensure_ascii=False,indent=2))
