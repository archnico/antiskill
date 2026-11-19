#!/usr/bin/env python3
import csv
import sqlite3
import tempfile
import unittest
import zipfile
from pathlib import Path

from ingest import build

FIELDS = ['Nominativo','ID FSI','FIN','Federazione','Anno Nascita',
          'Categoria','Anno Ultimo Tesseramento','Circolo Attuale',
          'Circolo Anno Precedente','Circolo Due Anni Precedenti',
          'Data Nascita','Provincia Residenza']

class IdentityImportTests(unittest.TestCase):
    def test_conflict_review_and_privacy(self):
        with tempfile.TemporaryDirectory() as d:
            d=Path(d); allin=d/'allin.csv'; fide=d/'fide.zip'; out=d/'result.sqlite3'
            rows=[
                ['Mario Rossi','100','10','ITA','1975','1N','2026','17009','17008','','1975-04-02','FI'],
                ['Gianni Verdi','200','20','ITA','1980','','2026','12001','','','',''],
                ['Gianni Verdi','200','21','ITA','1980','','2026','12001','','','',''],
                ['Anna Neri','300','20','ITA','1982','','2026','','','','',''],
                ['Luca Bianchi','400','40','ITA','1993','','2026','','','','',''],
                ['Rita Lunga','500','','ITA','1996','','2026','13001','','','',''],
                ['Nino Anni','600','60','ITA','1980','','2026','','','','',''],
                ['Manca Fide','700','70','ITA','2001','','2026','','','','',''],
            ]
            with allin.open('w',encoding='cp1252',newline='') as f:
                w=csv.writer(f,delimiter=';'); w.writerow(FIELDS); w.writerows(rows)
            xml='''<?xml version="1.0" encoding="utf-8"?>
<players>
<player><fideid>10</fideid><name>Mario Rossi</name><country>ITA</country><birthday>1975</birthday></player>
<player><fideid>20</fideid><name>Gianni Verdi</name><country>ITA</country><birthday>1980</birthday></player>
<player><fideid>21</fideid><name>Gianni Verdi</name><country>ITA</country><birthday>1980</birthday></player>
<player><fideid>40</fideid><name>Lukas Bianchi</name><country>ITA</country><birthday>1993</birthday></player>
<player><fideid>60</fideid><name>Nino Anni</name><country>ITA</country><birthday>1981</birthday></player>
<player><fideid>999</fideid><name>Italia Solitaria</name><country>ITA</country><birthday>1990</birthday></player>
<player><fideid>800</fideid><name>Francia Altra</name><country>FRA</country><birthday>1990</birthday></player>
</players>'''
            with zipfile.ZipFile(fide,'w') as z: z.writestr('players.xml',xml)
            m=build(allin,fide,out)
            self.assertEqual(m['allin']['fsi_rows'],8)
            self.assertEqual(m['links'],{'VERIFIED':1,'CONFLICT':3,'REVIEW':2,'SOURCE_ASSERTED':1})
            self.assertEqual(m['counts']['fide_player'],6)
            con=sqlite3.connect(out)
            self.assertEqual(con.execute('select count(*) from club_membership where fsi_id=100').fetchone()[0],2)
            self.assertEqual(con.execute('select status from identity_link where fsi_id=100').fetchone()[0],'VERIFIED')
            self.assertEqual(con.execute('select status from identity_link where fsi_id=700').fetchone()[0],'SOURCE_ASSERTED')
            text=' '.join(r[0] for r in con.execute("select sql from sqlite_master where type='table'"))
            self.assertNotIn('Data Nascita',text)
            self.assertNotIn('Provincia Residenza',text)
            self.assertNotIn('1975-04-02',str(list(con.execute('select * from fsi_player'))))
            self.assertEqual(con.execute('pragma integrity_check').fetchone()[0],'ok')
            con.close()

if __name__=='__main__':
    unittest.main()
