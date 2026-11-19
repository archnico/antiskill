#!/usr/bin/env python3
"""Synthetic, offline, privacy-safe tests for the source discrepancy ledger."""
import json
import sqlite3
import tempfile
import unittest
from pathlib import Path

from source_discrepancies import (
    PROFILE, audit, connect_readonly, ensure_private_output, write_outputs,
)

SOURCES = {
    "allin-2026-04": "a" * 64,
    "fide-current": "b" * 64,
    "fsi-official-tesserato-2026-12": "c" * 64,
    "chess-results-synthetic": "d" * 64,
    "fsi-club-roster-2026": "e" * 64,
}
RULE = "manual_selection_three_source_exact_id_name_birthyear_v1"


class EvidenceAuditTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.dbpath = self.root / "synthetic.sqlite3"
        db = sqlite3.connect(self.dbpath)
        db.executescript("""
            CREATE TABLE source(source_key TEXT PRIMARY KEY, sha256 TEXT, source_role TEXT);
            CREATE TABLE fsi_fide_assertion(
                fsi_id INTEGER, fide_id INTEGER, source_key TEXT,
                PRIMARY KEY(fsi_id,fide_id));
            CREATE TABLE identity_link(
                fsi_id INTEGER, fide_id INTEGER, status TEXT,
                reason TEXT, source_key TEXT, PRIMARY KEY(fsi_id,fide_id));
            CREATE TABLE fsi_player(
                fsi_id INTEGER PRIMARY KEY, full_name TEXT, birth_year INTEGER,
                source_key TEXT);
            CREATE TABLE fide_player(
                fide_id INTEGER PRIMARY KEY, full_name TEXT, birth_year INTEGER,
                source_key TEXT);
            CREATE TABLE fsi_official_name_observation(
                fsi_id INTEGER, full_name TEXT, birth_year INTEGER,
                source_key TEXT, source_sha256 TEXT);
            CREATE TABLE external_id_pair_observation(
                fsi_id INTEGER, fide_id INTEGER, source_key TEXT,
                source_sha256 TEXT);
            CREATE TABLE externally_certified_crosswalk(
                fsi_id INTEGER, fide_id INTEGER,
                tournament_source_key TEXT, tournament_source_sha256 TEXT,
                official_fsi_source_key TEXT, official_fsi_source_sha256 TEXT,
                official_fide_source_key TEXT, official_fide_source_sha256 TEXT,
                certification_rule TEXT);
            CREATE TABLE crosswalk_candidate(
                fsi_id INTEGER, fide_id INTEGER, status TEXT);
        """)
        db.executemany(
            "INSERT INTO source VALUES (?,?,?)",
            [(k,v,"synthetic") for k,v in SOURCES.items()]
        )
        db.execute("INSERT INTO fsi_fide_assertion VALUES (11,22,'allin-2026-04')")
        db.execute("INSERT INTO identity_link VALUES (11,22,'VERIFIED','direct_crosswalk_and_fide_snapshot','allin-2026-04')")
        db.execute("INSERT INTO fsi_player VALUES (11,'TEST, ANNA',1990,'allin-2026-04')")
        db.execute("INSERT INTO fide_player VALUES (22,'Anna Test',1990,'fide-current')")
        db.execute("INSERT INTO crosswalk_candidate VALUES (33,44,'CANDIDATE_ONLY')")
        db.commit()
        db.close()

    def mutate(self, sql, params=()):
        db = sqlite3.connect(self.dbpath)
        try:
            db.execute(sql, params)
            db.commit()
        finally:
            db.close()

    def run_audit(self):
        with connect_readonly(self.dbpath) as db:
            return audit(db)

    def add_official(self, fsi_id=11, name="ANNA TEST", year=1990):
        self.mutate(
            "INSERT INTO fsi_official_name_observation VALUES (?,?,?,?,?)",
            (fsi_id, name, year, "fsi-club-roster-2026", SOURCES["fsi-club-roster-2026"]),
        )

    def add_certificate(self, certificate_sha=None):
        self.mutate(
            "INSERT INTO identity_link VALUES (12,23,'VERIFIED','official_fsi_and_fide_plus_explicit_chess_results_pair','chess-results-synthetic')")
        self.mutate(
            "INSERT INTO external_id_pair_observation VALUES (12,23,?,?)",
            ("chess-results-synthetic", SOURCES["chess-results-synthetic"]))
        self.mutate("""
            INSERT INTO externally_certified_crosswalk
            VALUES (?,?,?,?,?,?,?,?,?)
        """, (12,23,"chess-results-synthetic",SOURCES["chess-results-synthetic"],
              "fsi-official-tesserato-2026-12",
              certificate_sha or SOURCES["fsi-official-tesserato-2026-12"],
              "fide-current",SOURCES["fide-current"],RULE))

    def test_allin_fide_compatible_is_not_certified(self):
        report, details = self.run_audit()
        self.assertEqual(report["profile"], PROFILE)
        self.assertEqual(report["states"], {"COMPATIBLE_BUT_UNCERTIFIED": 1})
        self.assertEqual(report["unlinked_candidates_by_status"], {"CANDIDATE_ONLY": 1})
        self.assertEqual(details, [])

    def test_official_fsi_roster_match_does_not_certify_crosswalk(self):
        self.add_official()
        report, details = self.run_audit()
        self.assertEqual(report["allin_pairs_with_official_fsi_name_year_agreement"], 1)
        self.assertEqual(report["states"], {"COMPATIBLE_BUT_UNCERTIFIED": 1})
        self.assertEqual(details, [])

    def test_official_roster_disagreement_creates_review(self):
        self.add_official(name="ALTRO TEST")
        report, details = self.run_audit()
        self.assertEqual(report["states"], {"REVIEW_REQUIRED": 1})
        self.assertIn("FSI_OFFICIAL_NAME_DISAGREEMENT", details[0]["reason_codes"])

    def test_names_differ_without_leaking_person_data(self):
        self.mutate("UPDATE fide_player SET full_name='SOMEONE ELSE' WHERE fide_id=22")
        report, details = self.run_audit()
        self.assertEqual(report["states"], {"REVIEW_REQUIRED": 1})
        self.assertIn("FSI_FIDE_NAME_DISAGREEMENT", details[0]["reason_codes"])
        self.assertNotIn("SOMEONE", json.dumps((report,details)))
        self.assertNotIn("ANNA", json.dumps((report,details)))
        self.assertNotIn("1990", json.dumps((report,details)))

    def test_missing_fide_id_is_review_not_false_proof(self):
        self.mutate("DELETE FROM fide_player WHERE fide_id=22")
        report, details = self.run_audit()
        self.assertEqual(report["states"], {"REVIEW_REQUIRED": 1})
        self.assertIn("FIDE_ID_NOT_IN_CHECKED_SNAPSHOT", details[0]["reason_codes"])

    def test_non_bijective_assertion_is_conflict(self):
        self.mutate("INSERT INTO fsi_fide_assertion VALUES (11,23,'allin-2026-04')")
        self.mutate("INSERT INTO fide_player VALUES (23,'ANNA TEST',1990,'fide-current')")
        report, details = self.run_audit()
        self.assertEqual(report["states"], {"CONFLICT": 2})
        self.assertTrue(all("ALLIN_NON_BIJECTIVE_ID" in d["reason_codes"] for d in details))

    def test_valid_independent_crosswalk_absent_from_allin_is_not_an_error(self):
        self.add_certificate()
        report, details = self.run_audit()
        self.assertEqual(report["states"]["INDEPENDENTLY_CERTIFIED_OUTSIDE_ALLIN"], 1)
        self.assertEqual(report["states"]["COMPATIBLE_BUT_UNCERTIFIED"], 1)
        self.assertEqual(details[0]["reason_codes"], ["PAIR_ABSENT_FROM_ALLIN"])

    def test_three_source_certificate_for_allin_pair_is_separate_status(self):
        self.mutate(
            "INSERT INTO external_id_pair_observation VALUES (11,22,?,?)",
            ("chess-results-synthetic", SOURCES["chess-results-synthetic"]))
        self.mutate("""
            INSERT INTO externally_certified_crosswalk VALUES (?,?,?,?,?,?,?,?,?)
        """, (11,22,"chess-results-synthetic",SOURCES["chess-results-synthetic"],
              "fsi-official-tesserato-2026-12",
              SOURCES["fsi-official-tesserato-2026-12"],
              "fide-current",SOURCES["fide-current"],RULE))
        report, details = self.run_audit()
        self.assertEqual(report["states"], {"INDEPENDENTLY_CERTIFIED": 1})
        self.assertEqual(details, [])

    def test_explicit_external_pair_conflict_has_higher_priority(self):
        self.mutate(
            "INSERT INTO external_id_pair_observation VALUES (11,44,?,?)",
            ("chess-results-synthetic", SOURCES["chess-results-synthetic"]))
        report, details = self.run_audit()
        self.assertEqual(report["states"], {"CONFLICT": 1})
        self.assertIn("INDEPENDENT_ID_PAIR_FSI_CONFLICT", details[0]["reason_codes"])

    def test_invalid_certificate_hash_is_blocked(self):
        self.add_certificate(certificate_sha="f" * 64)
        report, details = self.run_audit()
        self.assertEqual(report["states"]["CERTIFICATE_INVALID_REVIEW"], 1)
        self.assertIn("CERTIFICATE_SOURCE_HASH_CHANGED", details[0]["reason_codes"])
        self.assertNotIn("INDEPENDENTLY_CERTIFIED_OUTSIDE_ALLIN", report["states"])

    def test_certificate_without_event_pair_is_blocked(self):
        self.add_certificate()
        self.mutate("DELETE FROM external_id_pair_observation")
        report, details = self.run_audit()
        self.assertEqual(report["states"]["CERTIFICATE_INVALID_REVIEW"], 1)
        self.assertIn("CERTIFICATE_TOURNAMENT_PAIR_MISSING", details[0]["reason_codes"])

    def test_external_observation_hash_mismatch_fails_closed(self):
        self.mutate("INSERT INTO external_id_pair_observation VALUES (11,22,'chess-results-synthetic',?)",
                    ("f" * 64,))
        with self.assertRaisesRegex(ValueError, "fingerprint mismatch"):
            self.run_audit()

    def test_missing_primary_source_fails_closed(self):
        self.mutate("DELETE FROM source WHERE source_key='fide-current'")
        with self.assertRaisesRegex(ValueError, "Missing required source snapshot"):
            self.run_audit()

    def test_empty_assertion_cannot_proclaim_zero_discrepancy(self):
        self.mutate("DELETE FROM fsi_fide_assertion")
        with self.assertRaisesRegex(ValueError, "No source ID assertions"):
            self.run_audit()

    def test_connection_is_readonly(self):
        with connect_readonly(self.dbpath) as db:
            with self.assertRaises(sqlite3.OperationalError):
                db.execute("DELETE FROM fsi_fide_assertion")

    def test_reports_immutable_and_private(self):
        report, details = self.run_audit()
        summary = self.root / "summary.json"
        detail_path = self.root / "details.jsonl"
        write_outputs(summary, detail_path, report, details)
        self.assertEqual(json.loads(summary.read_text())["states"],
                         {"COMPATIBLE_BUT_UNCERTIFIED":1})
        self.assertEqual(detail_path.read_text(), "")
        with self.assertRaisesRegex(ValueError, "NEW"):
            write_outputs(summary, detail_path, report, details)
        public = Path(__file__).resolve().parent / "bad_audit.json"
        with self.assertRaisesRegex(ValueError, "outside public repository"):
            ensure_private_output(public)


if __name__ == "__main__":
    unittest.main()
