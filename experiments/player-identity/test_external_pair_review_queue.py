import csv
import sqlite3
import tempfile
import unittest
from pathlib import Path
from external_pair_review_queue import review,safe_cell

class ExternalPairReviewTests(unittest.TestCase):
    def make_db(self):
        db=sqlite3.connect(":memory:")
        db.row_factory=sqlite3.Row
        db.executescript("""
            CREATE TABLE source(source_key TEXT PRIMARY KEY,sha256 TEXT);
            INSERT INTO source VALUES('fixture','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');
            CREATE TABLE external_id_pair_observation(
              source_key TEXT,tournament_id INTEGER,fsi_id INTEGER,
              fide_id INTEGER,pair_status TEXT,source_url TEXT,source_sha256 TEXT);
            CREATE TABLE identity_link(fsi_id INTEGER,fide_id INTEGER,status TEXT,source_key TEXT);
            CREATE TABLE crosswalk_candidate(fsi_id INTEGER,fide_id INTEGER);
            CREATE TABLE fide_player(fide_id INTEGER,source_key TEXT);
            CREATE TABLE fsi_official_name_observation(fsi_id INTEGER);
            CREATE TABLE fsi_player(fsi_id INTEGER,source_key TEXT);
        """)
        for fsi in range(100,105):
            db.execute("INSERT INTO external_id_pair_observation VALUES(?,?,?,?,?,?,?)",
                       ("fixture",50,fsi,fsi+100,"MATCHES_CANDIDATE",
                        "https://example.test/results","a"*64))
        db.execute("INSERT INTO crosswalk_candidate VALUES(100,200)")
        db.execute("INSERT INTO identity_link VALUES(101,201,'VERIFIED','fixture')")
        db.execute("INSERT INTO identity_link VALUES(102,999,'VERIFIED','fixture')")
        db.execute("INSERT INTO identity_link VALUES(104,204,'REVIEW','fixture')")
        db.commit()
        return db

    def test_live_review_state_overrides_historical_status_without_mutating_evidence(self):
        with tempfile.TemporaryDirectory() as tmp:
            base=Path(tmp)
            db=self.make_db()
            output=review(db,base/"review.csv",base/"summary.json")
            self.assertEqual(output["distinct_explicit_id_pairs"],5)
            self.assertEqual(output["current_statuses"],{
                "CANDIDATE_WITH_EXPLICIT_IDS":1,
                "VERIFIED_NOW":1,
                "CONFLICT_REVIEW":1,
                "UNLINKED_WITH_EXPLICIT_IDS":1,
                "DISPUTED_EXISTING_LINK":1
            })
            with (base/"review.csv").open(newline="") as f:
                rows=list(csv.DictReader(f))
            self.assertEqual(rows[0]["current_status"],"CONFLICT_REVIEW")
            self.assertEqual(rows[-1]["current_status"],"VERIFIED_NOW")
            self.assertEqual(db.execute("SELECT COUNT(*) FROM identity_link").fetchone()[0],3)
            self.assertEqual(db.execute(
                "SELECT COUNT(*) FROM external_id_pair_observation WHERE pair_status='MATCHES_CANDIDATE'"
            ).fetchone()[0],5)
            db.execute("INSERT INTO identity_link VALUES(100,200,'VERIFIED','fixture')")
            output=review(db,base/"review.csv",base/"summary.json")
            self.assertEqual(output["current_statuses"]["VERIFIED_NOW"],2)
            db.close()

    def test_reject_source_sha_tampering(self):
        with tempfile.TemporaryDirectory() as tmp:
            db=self.make_db()
            db.execute("UPDATE source SET sha256=? WHERE source_key='fixture'",("b"*64,))
            with self.assertRaisesRegex(ValueError,"fingerprint"):
                review(db,Path(tmp)/"review.csv",Path(tmp)/"report.json")
            db.close()

    def test_csv_formula_mitigation(self):
        self.assertEqual(safe_cell('=1+1'),"'=1+1")
        self.assertEqual(safe_cell('+cmd'),"'+cmd")
        self.assertEqual(safe_cell('normal'),'normal')

if __name__=="__main__":
    unittest.main()
