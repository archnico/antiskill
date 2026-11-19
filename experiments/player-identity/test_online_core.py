import csv
import sqlite3
import tempfile
import unittest
from pathlib import Path

from ingest import build
from online_candidates import import_candidates
from core_overlay import assemble, validate_core_view
from player_lookup import profile

FIELDS=['Nominativo','ID FSI','FIN','Federazione','Anno Nascita',
        'Categoria','Anno Ultimo Tesseramento','Circolo Attuale',
        'Circolo Anno Precedente','Circolo Due Anni Precedenti',
        'Data Nascita','Provincia Residenza']

class AccountAndCoreOverlayTests(unittest.TestCase):
    def make_dbs(self,home):
        home=Path(home)
        f=home/'allin.csv'
        with f.open('w',encoding='cp1252',newline='') as out:
            w=csv.writer(out,delimiter=';');w.writerow(FIELDS)
            w.writerows([
              ['ROSSI MARIO','100','10001','ITA','1980','','2026','17009','','','',''],
              ['NOME IN DUBBIO','101','10002','ITA','1980','','2026','17009','','','',''],
            ])
        d=home/'identity.sqlite3'
        build(f,None,d)
        con=sqlite3.connect(d)
        con.execute('UPDATE identity_link SET status="VERIFIED" WHERE fsi_id=100')
        con.execute('UPDATE identity_link SET status="REVIEW" WHERE fsi_id=101')
        con.execute("INSERT INTO fide_player VALUES (10001,'Rossi Mario','ITA',1980,'','synthetic')")
        con.execute("INSERT INTO fide_player VALUES (10002,'Name Disputed','ITA',1980,'','synthetic')")
        con.commit();con.close()
        source=home/'fidescout.sqlite3'
        c=sqlite3.connect(source)
        c.executescript("""
          CREATE TABLE player(id INTEGER PRIMARY KEY,fide_id TEXT);
          CREATE TABLE account(id INTEGER PRIMARY KEY,player_id INTEGER,platform TEXT,
                              username TEXT,status TEXT);
          CREATE TABLE account_evidence(id INTEGER PRIMARY KEY,account_id INTEGER,direction TEXT,
                              evidence_type TEXT,strength TEXT,url TEXT,fetch_id INTEGER);
          INSERT INTO player VALUES (1,'10001'),(2,'10002');
          INSERT INTO account VALUES (1,1,'lichess','not-verified','HIGH');
          INSERT INTO account VALUES (2,2,'lichess','not-verifiable','VERIFIED');
          INSERT INTO account_evidence VALUES (1,1,'for','explicit_coach_assertion','strong',NULL,NULL);
          INSERT INTO account_evidence VALUES (2,2,'against','country_mismatch','weak',NULL,NULL);
        """)
        c.commit();c.close()
        return d,source

    def test_account_candidates_do_not_become_verified_even_with_upstream_verified(self):
        with tempfile.TemporaryDirectory() as temp:
            d,source=self.make_dbs(temp)
            report=Path(temp)/'accounts.json'
            result=import_candidates(d,source,report)
            self.assertEqual(result['accounts_observed'],2)
            self.assertEqual(result['verified_online_accounts_created'],0)
            self.assertEqual(result['evidence_observations'],2)
            with sqlite3.connect(d) as con:
                self.assertEqual(con.execute(
                    "SELECT count(*) FROM online_account_candidate WHERE link_status='IN_SCOPE_UNCONFIRMED'"
                ).fetchone()[0],2)
                self.assertEqual(con.execute(
                    "SELECT count(*) FROM online_account_proof"
                ).fetchone()[0],2)
                self.assertEqual(con.execute('PRAGMA integrity_check').fetchone()[0],'ok')
            with sqlite3.connect(d) as con:
                asserted=profile(con,'fsi',100)
                self.assertEqual(len(asserted['online_account_candidates']),1)
                self.assertEqual(asserted['online_account_candidates'][0]['link_status'],'IN_SCOPE_UNCONFIRMED')
                disputed=profile(con,'fsi',101)
                self.assertEqual(disputed['online_account_candidates'],[])
                from_fide=profile(con,'fide',10002)
                self.assertEqual(len(from_fide['online_account_candidates']),1)
            again=import_candidates(d,source,report)
            self.assertEqual(again['source_key'],result['source_key'])
            with sqlite3.connect(d) as con:
                self.assertEqual(con.execute('SELECT count(*) FROM online_account_candidate').fetchone()[0],2)

    def test_core_overlay_uses_only_verified_crosswalk_and_official_game_occurrences(self):
        with tempfile.TemporaryDirectory() as temp:
            d,_=self.make_dbs(temp)
            view={'profile':'antiskill_player_identity_view_v1',
                  'fide_id':'10001','collection_blake3':'a'*64,
                  'player_entity_id':'fide-10001',
                  'claims':[{'occurrence_id':'g1-'+('a'*64), 'side':'white',
                             'source_blake3':'b'*64}],
                  'candidates':[{'occurrence_id':'g1-'+('b'*64),'side':'white'}],
                  'conflicts':[]}
            with sqlite3.connect(d) as db:
                overlay=assemble(db,view,10001)
                self.assertEqual(overlay['fsi_id'],100)
                self.assertEqual(overlay['fsi_relation'],'VERIFIED_ID_CROSSWALK')
                self.assertEqual(overlay['core_occurrence_count'],1)
                self.assertEqual(overlay['core_occurrences'][0]['occurrence_id'],'g1-'+('a'*64))
                view['fide_id']='10002'
                overlay=assemble(db,view,10002)
                self.assertIsNone(overlay['fsi_id'])
                self.assertEqual(overlay['fsi_relation'],'REVIEW_REQUIRED')
            view['fide_id']='10001'
            view['claims'][0]['occurrence_id']='bad/id'
            with self.assertRaises(ValueError):
                validate_core_view(view,10001)

if __name__=='__main__':unittest.main()
