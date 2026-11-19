import sqlite3
import unittest
from fsi_roster_probe import parse_roster,check_candidate

def html(fsi_id, name, year):
    return (f'<div>Trovati <B>1</B> giocatori.</div>'
            f'<A HREF=https://www.torneionline.com/giocatori.php?tipo=11&ifsi={fsi_id} '
            f'title="Rating">{fsi_id}</A>'
            f'<!-- nome giocatore --><B>&nbsp;{name}</B>'
            f'<!-- dati giocatore --> Nato nel&nbsp;<B>{year}</B>').encode()

class FsiRosterTests(unittest.TestCase):
    def test_parse_anchored_exact_fsi_id_and_birth_year(self):
        self.assertEqual(parse_roster(html(100,'ROSSI MARIO',1984),100),
                         ('PARSED','ROSSI MARIO',1984))
        # Official FSI roster says "Nata nel" for female members.
        female=html(100,'VERDI ELENA',1986).replace(b'Nato nel',b'Nata nel')
        self.assertEqual(parse_roster(female,100),('PARSED','VERDI ELENA',1986))
        self.assertEqual(parse_roster(html(101,'ROSSI MARIO',1984),100)[0],
                         'FSI_ROSTER_UNPARSEABLE')
        self.assertEqual(parse_roster(b'Trovati <B>0</B> giocatori.',100)[0],
                         'FSI_ROSTER_NOT_LISTED')

    def test_corrob_only_never_upgrades_crosswalk(self):
        db=sqlite3.connect(':memory:')
        db.execute("""CREATE TABLE official_fsi_roster_probe(
          candidate_id TEXT,observed_year INT,fsi_id INT,official_name TEXT,
          official_birth_year INT,check_status TEXT,source_url TEXT,
          source_sha256 TEXT,observed_at TEXT,
          PRIMARY KEY(candidate_id,observed_year,source_sha256))""")
        db.execute('CREATE TABLE identity_link(fsi_id INT,fide_id INT,status TEXT)')
        record={'candidate_id':'fixture-1','fsi_id':100,
                'name_signature':'mario rossi','birth_year':1984}
        status=check_candidate(db,record,html(100,'MARIO ROSSI',1984),'https://example.test',2026)
        self.assertEqual(status,'FSI_ROSTER_CORROBORATED')
        self.assertEqual(db.execute('SELECT count(*) FROM identity_link').fetchone()[0],0)
        status=check_candidate(db,record,html(100,'MARIO ROSSI',1985),'https://example.test',2026)
        self.assertEqual(status,'FSI_ROSTER_CONFLICT')
        db.close()

if __name__=='__main__':
    unittest.main()
