import sqlite3
import tempfile
import unittest
from pathlib import Path

from audit_external_conflict import record_conflict
from chess_results_pairs import import_tournament
from test_chess_results_pairs import create_db,tournament_html,roster

class ExternalConflictAuditTests(unittest.TestCase):
    def test_birth_year_disagreement_remains_review_and_is_idempotent(self):
        with tempfile.TemporaryDirectory() as root:
            p=Path(root);dbpath=p/"identity.sqlite3"
            create_db(dbpath)
            with sqlite3.connect(dbpath) as db:
                db.execute("""INSERT INTO identity_link
                 VALUES(3001,900000001,'REVIEW','birth_year_disagrees','allin')""")
            f=p/"tnr1364644.html";f.write_bytes(tournament_html())
            report=import_tournament(dbpath,1364644,f,p/"tournament-report.json")
            self.assertEqual(report["classifications"]["AGREES_DISPUTED"],1)
            official=p/"fsi-member.html";official.write_bytes(roster(year=2004))
            result=record_conflict(dbpath,3001,900000001,official,2026,
                                   p/"conflict-report.json")
            self.assertEqual(result["status"],"REVIEW_REQUIRED")
            self.assertEqual(result["conflict_type"],"BIRTH_YEAR_DISAGREEMENT")
            second=record_conflict(dbpath,3001,900000001,official,2026,
                                   p/"conflict-report.json")
            self.assertEqual(second["status"],"REVIEW_REQUIRED")
            with sqlite3.connect(dbpath) as db:
                self.assertEqual(db.execute("SELECT status FROM identity_link WHERE fsi_id=3001").fetchone()[0],"REVIEW")
                self.assertEqual(db.execute("SELECT count(*) FROM crosswalk_conflict_evidence").fetchone()[0],1)
                self.assertEqual(db.execute("PRAGMA integrity_check").fetchone()[0],"ok")

    def test_matching_official_birth_year_is_not_a_conflict(self):
        with tempfile.TemporaryDirectory() as root:
            p=Path(root);dbpath=p/"identity.sqlite3";create_db(dbpath)
            with sqlite3.connect(dbpath) as db:
                db.execute("""INSERT INTO identity_link
                 VALUES(3001,900000001,'REVIEW','birth_year_disagrees','allin')""")
            f=p/"tnr1364644.html";f.write_bytes(tournament_html())
            import_tournament(dbpath,1364644,f,p/"tournament-report.json")
            official=p/"fsi-member.html";official.write_bytes(roster(year=2003))
            with self.assertRaisesRegex(ValueError,"No discrepancy"):
                record_conflict(dbpath,3001,900000001,official,2026,
                                p/"conflict-report.json")

if __name__=="__main__":
    unittest.main()
