import hashlib
import sqlite3
import tempfile
import unittest
from pathlib import Path

from fsi_official_roster_ingest import import_official

def synthetic_roster():
    records=[]
    for fsi,name,birth in [(1001,"ROSSI MARIO",1975),(1002,"VERDI ELENA",2009)]:
        row=[str(fsi),name,"","2026","M",str(birth),"NC","FI",
             "TOSCANA","ITA","5004","CIRCOLO TEST","FI","TOS","TA 100"]
        records.append("<tr>"+"".join("<td>"+v+"</td>" for v in row)+"</tr>")
    return ("<div>Trovati <B>2</B> giocatori.</div>"
            "<div class=\"table-responsive\"><table><tr><th>Id</th></tr>"+
            "".join(records)+"</table></div>").encode()

class OfficialFsiRosterImportTests(unittest.TestCase):
    def test_adds_missing_people_but_not_fide_ids_or_duplicate_memberships(self):
        with tempfile.TemporaryDirectory() as root:
            base=Path(root);f=base/"fsi-club-roster-2026-5004.html"
            f.write_bytes(synthetic_roster())
            sha=hashlib.sha256(f.read_bytes()).hexdigest()
            key="fsi-club-roster-2026-5004-"+sha[:12]
            dbpath=base/"people.sqlite3"
            with sqlite3.connect(dbpath) as db:
                db.executescript("""
                  CREATE TABLE source(source_key TEXT PRIMARY KEY,url TEXT,sha256 TEXT,
                    bytes INTEGER,retrieved_at TEXT,source_role TEXT);
                  CREATE TABLE fsi_player(fsi_id INTEGER PRIMARY KEY,full_name TEXT,birth_year INTEGER,
                    federation TEXT,category TEXT,latest_member_year INTEGER,
                    reported_fide_id INTEGER,source_key TEXT);
                  CREATE TABLE club_membership(fsi_id INTEGER,club_code INTEGER,membership_year INTEGER,
                    evidence_kind TEXT,source_key TEXT,
                    PRIMARY KEY(fsi_id,club_code,membership_year,evidence_kind));
                  CREATE TABLE fsi_official_club_roster_snapshot(observed_year INTEGER,
                    club_code INTEGER,source_key TEXT,source_sha256 TEXT,parsed_members INTEGER);
                  CREATE TABLE fsi_official_club_roster(fsi_id INTEGER,observed_year INTEGER,
                    club_code INTEGER,source_key TEXT,source_sha256 TEXT);
                  CREATE TABLE identity_link(fsi_id INTEGER,fide_id INTEGER,status TEXT);
                """)
                db.execute("INSERT INTO source VALUES (?,?,?,?,?,?)",
                           (key,"https://example.test",sha,f.stat().st_size,"2026-11-19","synthetic"))
                db.execute("INSERT INTO fsi_official_club_roster_snapshot VALUES (?,?,?,?,?)",
                           (2026,5004,key,sha,2))
                db.executemany("INSERT INTO fsi_official_club_roster VALUES (?,?,?,?,?)",
                               [(fsi,2026,5004,key,sha) for fsi in (1001,1002)])
                db.execute("INSERT INTO fsi_player VALUES (?,?,?,?,?,?,?,?)",
                           (1001,"ROSSI MARIO",1975,None,None,2025,None,"allin"))
            result=import_official(dbpath,base,base/"report.json")
            self.assertEqual(result["new_fsi_records"],1)
            self.assertEqual(result["new_official_club_membership_observations"],2)
            self.assertEqual(result["fide_links_created"],0)
            with sqlite3.connect(dbpath) as db:
                self.assertEqual(db.execute("SELECT count(*) FROM fsi_player").fetchone()[0],2)
                self.assertEqual(db.execute("SELECT reported_fide_id FROM fsi_player WHERE fsi_id=1002").fetchone()[0],None)
                self.assertEqual(db.execute("SELECT count(*) FROM club_membership").fetchone()[0],2)
                self.assertEqual(db.execute("SELECT count(*) FROM identity_link").fetchone()[0],0)
            again=import_official(dbpath,base,base/"report2.json")
            self.assertNotIn("new_fsi_records",again)
            self.assertNotIn("new_official_club_membership_observations",again)
            with sqlite3.connect(dbpath) as db:
                self.assertEqual(db.execute("SELECT count(*) FROM fsi_official_name_observation").fetchone()[0],2)

    def test_rejects_modified_raw_snapshot(self):
        with tempfile.TemporaryDirectory() as root:
            base=Path(root);f=base/"fsi-club-roster-2026-5004.html"
            f.write_bytes(synthetic_roster())
            dbpath=base/"bad.sqlite3"
            with sqlite3.connect(dbpath) as db:
                db.execute("""CREATE TABLE fsi_official_club_roster_snapshot(
                    observed_year INTEGER,club_code INTEGER,source_key TEXT,
                    source_sha256 TEXT,parsed_members INTEGER)""")
                db.execute("INSERT INTO fsi_official_club_roster_snapshot VALUES(?,?,?,?,?)",
                           (2026,5004,"source","a"*64,2))
            with self.assertRaisesRegex(ValueError,"changed"):
                import_official(dbpath,base,base/"bad-report.json")

if __name__=="__main__":
    unittest.main()
