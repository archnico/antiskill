import csv
import sqlite3
import tempfile
import unittest
from pathlib import Path
from candidate_review_queue import extract, csv_safe

class CandidateReviewQueueTests(unittest.TestCase):
    def test_priority_and_no_promotion(self):
        with tempfile.TemporaryDirectory() as root:
            p=Path(root)
            with sqlite3.connect(p/"db.sqlite3") as db:
                db.executescript("""
                  CREATE TABLE crosswalk_candidate(
                    candidate_id TEXT,fsi_id INTEGER,fide_id INTEGER,name_signature TEXT,
                    birth_year INTEGER,fsi_source_sha256 TEXT,fide_source_sha256 TEXT,status TEXT
                  );
                  CREATE TABLE official_fsi_roster_probe(
                    candidate_id TEXT,observed_year INTEGER,official_name TEXT,
                    source_url TEXT,source_sha256 TEXT,check_status TEXT,observed_at TEXT
                  );
                  CREATE TABLE identity_link(fsi_id INTEGER,fide_id INTEGER,status TEXT);
                """)
                for i in range(1,4):
                    db.execute("INSERT INTO crosswalk_candidate VALUES (?,?,?,?,?,?,?,?)",
                       (f"cand{i}",i,i+100,f"person {i}",1980,
                        "a"*64,"b"*64,"CANDIDATE_ONLY"))
                db.execute("INSERT INTO official_fsi_roster_probe VALUES (?,?,?,?,?,?,?)",
                    ("cand1",2026,"Name Alpha","https://example.org","c"*64,
                     "FSI_ROSTER_CORROBORATED","2026-11-19"))
                db.execute("INSERT INTO official_fsi_roster_probe VALUES (?,?,?,?,?,?,?)",
                    ("cand2",2026,"Name Beta","https://example.org","d"*64,
                     "FSI_ROSTER_CONFLICT","2026-11-19"))
                out=extract(db,p/"queue.csv",p/"summary.json")
                self.assertEqual(out['candidates'],3)
                self.assertEqual(out['links_promoted'],0)
                self.assertEqual(db.execute("SELECT count(*) FROM identity_link").fetchone()[0],0)
                with (p/"queue.csv").open(newline="") as f:
                    rows=list(csv.DictReader(f))
                self.assertEqual([int(x["fsi_id"]) for x in rows],[2,1,3])
                self.assertEqual(rows[0]["fsi_roster_status"],"FSI_ROSTER_CONFLICT")
                self.assertEqual(rows[1]["fsi_roster_status"],"FSI_ROSTER_CORROBORATED")
                self.assertEqual(rows[2]["fsi_roster_status"],"FSI_ROSTER_NOT_CHECKED")
        self.assertEqual(csv_safe('=HYPERLINK("x")'),'\'=HYPERLINK("x")')

if __name__=="__main__":
    unittest.main()
