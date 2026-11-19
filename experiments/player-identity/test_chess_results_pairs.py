import sqlite3
import tempfile
import unittest
from pathlib import Path

from chess_results_pairs import import_tournament,parse_tournament
from certify_external_pair import certify

def tournament_html():
    rows=[]
    for i in range(1,5):
        rows.append(f"<tr><td>{i}</td><td>Reference, Player{i}</td><td>{1000+i}</td><td>{2000+i}</td><td>ITA</td></tr>")
    rows.append("<tr><td>5</td><td>Vaporini, Lucio</td><td>3001</td><td>900000001</td><td>ITA</td></tr>")
    return ("<table><tr><th>N°</th><th>Nom</th><th>ID</th><th>ID FIDE</th><th>FED</th></tr>"
            +"".join(rows)+"</table>").encode()

def roster(name="VAPORINI LUCIO",year=2003):
    raw=(f"<input type=hidden name=anno value=2026>"
         f"<div>Trovati <B>1</B> giocatori.</div>"
         f"<A HREF=https://www.torneionline.com/giocatori.php?tipo=11&ifsi=3001>3001</A>"
         f"<!-- nome giocatore --><B>&nbsp;{name}</B>"
         f"<!-- dati giocatore --> Nato nel&nbsp;<B>{year}</B>").encode()
    return raw+b" "*20_000

def create_db(p):
    db=sqlite3.connect(p)
    db.executescript("""
      CREATE TABLE source(source_key TEXT PRIMARY KEY,url TEXT,sha256 TEXT,bytes INTEGER,
          retrieved_at TEXT,source_role TEXT);
      CREATE TABLE fsi_player(fsi_id INTEGER PRIMARY KEY,full_name TEXT,birth_year INTEGER,
          federation TEXT,category TEXT,latest_member_year INTEGER,
          reported_fide_id INTEGER,source_key TEXT);
      CREATE TABLE fide_player(fide_id INTEGER PRIMARY KEY,full_name TEXT,
          fed TEXT,birth_year INTEGER,title TEXT,source_key TEXT);
      CREATE TABLE identity_link(fsi_id INTEGER,fide_id INTEGER,status TEXT,reason TEXT,
          source_key TEXT, PRIMARY KEY(fsi_id,fide_id));
      CREATE TABLE crosswalk_candidate(fsi_id INTEGER,fide_id INTEGER);
    """)
    db.execute("INSERT INTO source VALUES (?,?,?,?,?,?)",
               ("fide-current","official FIDE","f"*64,5000,"2026-11-19","official"))
    for i in range(1,5):
        db.execute("INSERT INTO identity_link VALUES (?,?,?,'baseline','allin')",
                   (1000+i,2000+i,"VERIFIED"))
    db.execute("INSERT INTO fide_player VALUES (?,?,?,?,?,?)",
               (900000001,"Vaporini, Lucio","ITA",2003,"IM","fide-current"))
    db.commit();db.close()

class SwissManagerCrosswalkTests(unittest.TestCase):
    def test_three_way_explicit_certification_is_idempotent(self):
        with tempfile.TemporaryDirectory() as root:
            p=Path(root)
            dbpath=p/"id.sqlite3";create_db(dbpath)
            f=p/"tnr1364644.html";f.write_bytes(tournament_html())
            self.assertEqual(len(parse_tournament(f.read_bytes())),5)
            summary=import_tournament(dbpath,1364644,f,p/"report.json")
            self.assertEqual(summary["baseline_verified"],4)
            self.assertEqual(summary["classifications"]["UNLINKED_ID_PAIR"],1)
            official=p/"official.html";official.write_bytes(roster())
            result=certify(dbpath,3001,900000001,official,2026)
            self.assertEqual(result["new_verified_links"],1)
            self.assertEqual(result["status"],"VERIFIED_FROM_THREE_SOURCES")
            repeated=certify(dbpath,3001,900000001,official,2026)
            self.assertEqual(repeated["status"],"ALREADY_CERTIFIED")
            with sqlite3.connect(dbpath) as db:
                self.assertEqual(db.execute("SELECT status FROM identity_link WHERE fsi_id=3001").fetchone()[0],"VERIFIED")
                self.assertEqual(db.execute("SELECT count(*) FROM externally_certified_crosswalk").fetchone()[0],1)
                self.assertEqual(db.execute("SELECT birth_year FROM fsi_player WHERE fsi_id=3001").fetchone()[0],2003)
                self.assertEqual(db.execute("PRAGMA integrity_check").fetchone()[0],"ok")

    def test_candidate_label_is_not_automatic_but_explicit_three_way_proof_may_verify(self):
        with tempfile.TemporaryDirectory() as root:
            p=Path(root); dbpath=p/"id.sqlite3";create_db(dbpath)
            # A name/year candidate is not certified; it is merely a review lead.
            with sqlite3.connect(dbpath) as db:
                db.execute("INSERT INTO crosswalk_candidate VALUES (?,?)",
                           (3001,900000001))
                self.assertEqual(db.execute(
                    "SELECT count(*) FROM identity_link WHERE fsi_id=3001").fetchone()[0],0)
            f=p/"tnr1364644.html";f.write_bytes(tournament_html())
            summary=import_tournament(dbpath,1364644,f,p/"report.json")
            self.assertEqual(summary["classifications"]["MATCHES_CANDIDATE"],1)
            official=p/"official.html";official.write_bytes(roster())
            certified=certify(dbpath,3001,900000001,official,2026)
            self.assertEqual(certified["new_verified_links"],1)
            with sqlite3.connect(dbpath) as db:
                self.assertEqual(db.execute(
                    "SELECT count(*) FROM identity_link WHERE fsi_id=3001 AND status='VERIFIED'").fetchone()[0],1)

    def test_mismatched_primary_sources_fail_without_writes(self):
        with tempfile.TemporaryDirectory() as root:
            p=Path(root)
            dbpath=p/"id.sqlite3";create_db(dbpath)
            f=p/"tnr1364644.html";f.write_bytes(tournament_html())
            import_tournament(dbpath,1364644,f,p/"report.json")
            official=p/"official.html"
            official.write_bytes(roster(year=2004))
            with self.assertRaisesRegex(ValueError,"disagreement"):
                certify(dbpath,3001,900000001,official,2026)
            with sqlite3.connect(dbpath) as db:
                self.assertEqual(db.execute("SELECT count(*) FROM identity_link WHERE fsi_id=3001").fetchone()[0],0)
                self.assertEqual(db.execute("SELECT count(*) FROM externally_certified_crosswalk").fetchone()[0],0)
            official.write_bytes(roster(name="DIFFERENT PERSON"))
            with self.assertRaisesRegex(ValueError,"disagreement"):
                certify(dbpath,3001,900000001,official,2026)
            official.write_bytes(roster().replace(b"name=anno value=2026",
                                                  b"name=anno value=2025"))
            with self.assertRaisesRegex(ValueError,"does not declare requested year"):
                certify(dbpath,3001,900000001,official,2026)

    def test_foreign_player_national_ids_are_excluded_from_fsi_namespace(self):
        with tempfile.TemporaryDirectory() as root:
            p=Path(root); dbpath=p/"id.sqlite3";create_db(dbpath)
            foreign=(b"<tr><td>6</td><td>Foreign, Player</td>"
                     b"<td>99991</td><td>99992</td><td>GER</td></tr>")
            raw=tournament_html().replace(b"</table>",foreign+b"</table>")
            f=p/"tnr1364644.html";f.write_bytes(raw)
            result=import_tournament(dbpath,1364644,f,p/"report.json")
            self.assertEqual(result["foreign_national_id_rows_excluded"],1)
            self.assertEqual(result["explicit_pairs"],5)
            with sqlite3.connect(dbpath) as db:
                self.assertEqual(db.execute(
                   "SELECT count(*) FROM external_id_pair_observation WHERE fsi_id=99991"
                ).fetchone()[0],0)

    def test_nonitalian_national_id_semantics_fail(self):
        with tempfile.TemporaryDirectory() as root:
            p=Path(root);dbpath=p/"id.sqlite3";create_db(dbpath)
            f=p/"tnr1364644.html"
            f.write_bytes(tournament_html().replace(b"1001",b"9991"))
            with self.assertRaisesRegex(ValueError,"national ID semantics|National IDs"):
                import_tournament(dbpath,1364644,f,p/"report.json")

if __name__=="__main__":
    unittest.main()
