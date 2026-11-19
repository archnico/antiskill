import csv
import sqlite3
import tempfile
import unittest
from pathlib import Path

from crosswalk_candidates import run
from ingest import build

FIELDS=['Nominativo','ID FSI','FIN','Federazione','Anno Nascita',
        'Categoria','Anno Ultimo Tesseramento','Circolo Attuale',
        'Circolo Anno Precedente','Circolo Due Anni Precedenti',
        'Data Nascita','Provincia Residenza']

class CrosswalkCandidateTests(unittest.TestCase):
    def test_unlinked_full_corpus_unique_name_and_year_only(self):
        with tempfile.TemporaryDirectory() as root:
            d=Path(root); source=d/'allin.csv'; dest=d/'people.sqlite3'
            rows=[
                ['VAPORINI LUCIO','100','','ITA','1980','','2026','17009','','','',''],
                ['GELSI ALMA','101','','ITA','1981','','2026','17009','','','',''],
                ['GELSI ALMA','102','','ITA','1981','','2026','17009','','','',''],
                ['RUGIADA MARCO','103','99901','ITA','1975','','2026','17009','','','',''],
                ['SOLE LUNA','104','','ITA','1982','','2026','17009','','','',''],
            ]
            with source.open('w',encoding='cp1252',newline='') as f:
                w=csv.writer(f,delimiter=';');w.writerow(FIELDS);w.writerows(rows)
            build(source,None,dest)
            with sqlite3.connect(dest) as con:
                con.execute("INSERT INTO source VALUES (?,?,?,?,?,?)",
                  ('fide-current','synthetic','f'*64,42,'2026-11-19','test-only'))
                con.execute("INSERT INTO source VALUES (?,?,?,?,?,?)",
                  ('fsi-official-synthetic','fixture','e'*64,123,'2026-11-19','official'))
                con.execute("INSERT INTO fsi_player VALUES (?,?,?,?,?,?,?,?)",
                  (105,'RUBINO SCURO',1970,None,None,2026,None,'fsi-official-synthetic'))
                con.executemany("INSERT INTO fide_player VALUES (?,?,?,?,?,?)",[
                    (30001,'Lucio Vaporini','ITA',1980,'','fide-current'),
                    (30002,'Alma Gelsi','ITA',1981,'','fide-current'),
                    (99901,'Marco Rugiada','ITA',1975,'','fide-current'),
                    (30004,'Luna Sole','ITA',1982,'','fide-current'),
                    (30005,'Sole Luna','ITA',1982,'','fide-current'),
                    (30006,'Scuro Rubino','ITA',1970,'','fide-current'),
                ])
            report=d/'report.json'
            result=run(dest,report)
            self.assertEqual(result['candidate_pairs'],2)
            self.assertEqual(result['new_candidate_pairs_added'],2)
            self.assertEqual(result['ambiguous_name_year_signatures'],2)
            self.assertEqual(result['verified_links_created'],0)
            with sqlite3.connect(dest) as con:
                self.assertEqual(con.execute('SELECT fsi_id,fide_id,status FROM crosswalk_candidate ORDER BY fsi_id').fetchall(),
                                 [(100,30001,'CANDIDATE_ONLY'),(105,30006,'CANDIDATE_ONLY')])
                self.assertEqual(con.execute('SELECT count(*) FROM identity_link').fetchone()[0],1)
                self.assertEqual(con.execute('PRAGMA integrity_check').fetchone()[0],'ok')
            repeat=run(dest,report)
            self.assertEqual(repeat['new_candidate_pairs_added'],0)
            with sqlite3.connect(dest) as con:
                self.assertEqual(con.execute('SELECT count(*) FROM crosswalk_candidate').fetchone()[0],2)
                self.assertEqual(con.execute('SELECT fsi_source_sha256 FROM crosswalk_candidate WHERE fsi_id=105').fetchone()[0], 'e'*64)
                con.execute("INSERT INTO identity_link VALUES (?,?,?,?,?)",
                           (105,30006,'VERIFIED','explicit-third-party','synthetic-evidence'))
            retired=run(dest,report)
            self.assertEqual(retired['retired_after_explicit_verification'],1)
            self.assertEqual(retired['candidate_pairs'],1)
            with sqlite3.connect(dest) as con:
                self.assertEqual(con.execute('SELECT count(*) FROM crosswalk_candidate').fetchone()[0],1)
                self.assertEqual(con.execute('SELECT count(*) FROM crosswalk_candidate_retirement').fetchone()[0],1)

if __name__=='__main__':
    unittest.main()
