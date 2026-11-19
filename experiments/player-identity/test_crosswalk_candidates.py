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
                con.executemany("INSERT INTO fide_player VALUES (?,?,?,?,?,?)",[
                    (30001,'Lucio Vaporini','ITA',1980,'','fide-current'),
                    (30002,'Alma Gelsi','ITA',1981,'','fide-current'),
                    (99901,'Marco Rugiada','ITA',1975,'','fide-current'),
                    (30004,'Luna Sole','ITA',1982,'','fide-current'),
                    (30005,'Sole Luna','ITA',1982,'','fide-current'),
                ])
            report=d/'report.json'
            result=run(dest,report)
            self.assertEqual(result['candidate_pairs'],1)
            self.assertEqual(result['ambiguous_name_year_signatures'],2)
            self.assertEqual(result['verified_links_created'],0)
            with sqlite3.connect(dest) as con:
                self.assertEqual(con.execute('SELECT fsi_id,fide_id,status FROM crosswalk_candidate').fetchall(),
                                 [(100,30001,'CANDIDATE_ONLY')])
                self.assertEqual(con.execute('SELECT count(*) FROM identity_link').fetchone()[0],1)
                self.assertEqual(con.execute('PRAGMA integrity_check').fetchone()[0],'ok')
            run(dest,report)
            with sqlite3.connect(dest) as con:
                self.assertEqual(con.execute('SELECT count(*) FROM crosswalk_candidate').fetchone()[0],1)

if __name__=='__main__':
    unittest.main()
