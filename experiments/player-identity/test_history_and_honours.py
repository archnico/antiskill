import csv
import sqlite3
import tempfile
import unittest
from pathlib import Path

from ingest import build
from enrich_all_albi import import_all, parse_honour_rolls
from enrich_club_history import apply as import_history
from player_lookup import profile, find_names

HEADERS=['Nominativo','ID FSI','FIN','Federazione','Anno Nascita',
         'Categoria','Anno Ultimo Tesseramento','Circolo Attuale',
         'Circolo Anno Precedente','Circolo Due Anni Precedenti',
         'Data Nascita','Provincia Residenza']

def row(name,fsi,code='17009'):
    return [name,str(fsi),'','ITA','1975','','2026',code,code,'',
            '1975-04-02','FI']

def club_html(name,code,president,board):
    return (f'<h2><B>{name}</B></h2>'
            f'<div><B>Regione:</B>&nbsp;TOSCANA</div>'
            f'<div><B>Provincia:</B>&nbsp;Firenze</div>'
            f'<div><B>Presidente:</B>&nbsp;{president}</div>'
            f'<div><B>Id FSI:</B>&nbsp;{code}</div>'
            f'<div><B>Consiglio:</B>&nbsp;{board};</div>')

def award_html(name,event,year,first):
    cells=[event,'1',str(year),first,'Lucio Vaporini','Altra Persona','Firenze']
    return '<tr>'+''.join('<td>'+x+'</td>' for x in cells)+'</tr>'

class HistoricalIdentityTests(unittest.TestCase):
    def test_all_honours_and_historical_clubs_no_identity_merges(self):
        with tempfile.TemporaryDirectory() as td:
            d=Path(td); p=d/'allin.csv'
            with p.open('w',encoding='cp1252',newline='') as f:
                w=csv.writer(f,delimiter=';');w.writerow(HEADERS)
                w.writerow(row('ROSSI MARIO',100))
                w.writerow(row('VAPORINI LUCIO',101))
                w.writerow(row('VAPORINI LUCIO',102))
            db_path=d/'ident.sqlite3'
            build(p,None,db_path)
            c25=d/'fsi_societa_2025.html'
            c26=d/'fsi_societa_2026.html'
            c25.write_text(club_html('VECCHIO CIRCOLO',17009,'ROSSI MARIO','ROSSI MARIO'))
            c26.write_text(club_html('NUOVO CIRCOLO',17009,'ROSSI MARIO','VAPORINI LUCIO'))
            report=d/'clubs.json'
            result=import_history(db_path,[c25,c26],report)
            self.assertEqual(result['counts']['historical_name_records'],2)
            self.assertIn('CANDIDATE',str(result['counts']))
            with sqlite3.connect(db_path) as db:
                names=db.execute('SELECT official_name,observed_year FROM club_name_history ORDER BY observed_year').fetchall()
                self.assertEqual(names,[('VECCHIO CIRCOLO',2025),('NUOVO CIRCOLO',2026)])
                self.assertEqual(db.execute('select count(*) from club_office_observation where verification="AMBIGUOUS"').fetchone()[0],1)
                self.assertEqual(db.execute('select count(*) from club_office_observation where verification="CANDIDATE"').fetchone()[0],3)
            # Source is a fixture, not retrieved FSI HTML. Separate types are not conflated.
            albi=d/'all.html'
            records=[award_html('Lucio Vaporini','Campionato Italiano Assoluto',2025,'Lucio Vaporini'),
                     award_html('Team','Campionato Italiano a Squadre',2025,'NUOVO CIRCOLO'),
                     award_html('Prize','Premio dirigenti',2025,'Mario Rossi'),
                     award_html('Online','Campionato italiano on line Rapid',2025,'Mario Rossi')]
            records.extend(award_html('other','Campionato Under 20',1900+i,'Qualcuno') for i in range(97))
            albi.write_text('<table>'+''.join(records)+'</table>')
            self.assertEqual(len(parse_honour_rolls(albi.read_bytes())),101)
            report=d/'albi.json'
            m=import_all(db_path,albi,report)
            self.assertEqual(m['metrics']['source_rows'],101)
            with sqlite3.connect(db_path) as db:
                self.assertEqual(db.execute('SELECT count(*) FROM honour_roll_placement WHERE link_status="NOT_PERSON_TEAM_OR_SCHOOL"').fetchone()[0],3)
                self.assertEqual(db.execute('SELECT count(*) FROM honour_roll_placement WHERE link_status="NOT_PLAYER_WIN_INSTITUTIONAL"').fetchone()[0],3)
                self.assertTrue(db.execute('SELECT count(*) FROM honour_roll_placement WHERE link_status="AMBIGUOUS"').fetchone()[0]>=2)
                self.assertEqual(db.execute('PRAGMA integrity_check').fetchone()[0],'ok')
                before=db.execute('SELECT count(*) FROM honour_roll_placement').fetchone()[0]
            import_all(db_path,albi,report)
            with sqlite3.connect(db_path) as db:
                self.assertEqual(db.execute('SELECT count(*) FROM honour_roll_placement').fetchone()[0],before)

                dossier=profile(db,'fsi',100)
                self.assertTrue(dossier['found'])
                self.assertEqual(
                    {(r['membership_year'],r['official_name']) for r in dossier['membership_history']},
                    {(2025,'VECCHIO CIRCOLO'),(2026,'NUOVO CIRCOLO')})
                self.assertTrue(any(r['classification']=='online_individual'
                                    and r['placement']==1 for r in dossier['honour_roll_candidates']))
                self.assertFalse(any(r['classification']=='team_or_school'
                                     for r in dossier['honour_roll_candidates']))
                self.assertEqual(len(dossier['association_office_candidates']),3)
                self.assertEqual(len(find_names(db,'Lucio Vaporini')['matches']),2)
                # Two exact same-name FSI IDs must remain separate; no award assigned.
                ambiguous=profile(db,'fsi',101)
                self.assertEqual(ambiguous['honour_roll_candidates'],[])

if __name__=='__main__':
    unittest.main()
