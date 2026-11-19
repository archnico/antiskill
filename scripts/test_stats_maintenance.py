#!/usr/bin/env python3
import importlib.util
import json
import subprocess
import tempfile
import sqlite3
import unittest
from pathlib import Path

MODULE=Path(__file__).with_name("stats_maintenance.py")
spec=importlib.util.spec_from_file_location("maintenance",MODULE)
m=importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

class StatsMaintenanceTests(unittest.TestCase):
    def test_previous_month(self):
        self.assertEqual(m.previous_month("2026-01"),"2025-12")
        self.assertEqual(m.previous_month("2026-11"),"2026-10")

    def test_manifest_content_address_and_corruption_guard(self):
        with tempfile.TemporaryDirectory() as td:
            root=Path(td)
            key="fsi-members-region-tos-2026"
            body=b"123;ONE;TWO;2010;NC;FI;TOSCANA;ITA;17029;CLUB;FI;TOSCANA;TG;1;TG\n"
            digest=subprocess.check_output(["b3sum"],input=body).decode().split()[0]
            obj=root/"raw"/"objects"/"blake3"/digest[:2]/digest
            obj.parent.mkdir(parents=True)
            obj.write_bytes(body)
            manifest_path=root/"raw"/"manifests"/key/"2026.json"
            manifest_path.parent.mkdir(parents=True)
            manifest_path.write_text(json.dumps({"source_key":key,"blake3":digest,
                "object_path":str(obj.relative_to(root)),"bytes":len(body),"fetched_at_unix_ms":1795000000000}))
            ref=root/"raw"/"refs"/(key+".json")
            ref.parent.mkdir(parents=True)
            ref.write_text(json.dumps({"source_key":key,"blake3":digest,"manifest_path":str(manifest_path.relative_to(root))}))
            found=m.snapshot(root,key)
            self.assertEqual(found["digest"],digest)
            m.verify_digest(found)
            obj.write_bytes(body.replace(b"ONE",b"XXX"))
            with self.assertRaises(ValueError):m.verify_digest(found)

    def test_identity_export_is_versioned_and_rejects_duplicate_ambiguity(self):
        with tempfile.TemporaryDirectory() as td:
            root=Path(td)/"archive"
            root.mkdir()
            registry=Path(td)/"registry"
            registry.mkdir()
            db=registry/"identity_registry_2026-11-19.sqlite3"
            with sqlite3.connect(db) as con:
                con.execute("CREATE TABLE identity_link(fsi_id INTEGER, fide_id INTEGER, status TEXT)")
                con.execute("INSERT INTO identity_link VALUES (101, 201, 'VERIFIED')")
                con.execute("INSERT INTO identity_link VALUES (102, 202, 'REVIEW')")
            first=m.identity_export(root,registry)
            self.assertTrue(first.exists())
            self.assertEqual(first,m.identity_export(root,registry))
            first_bytes=first.read_bytes()
            with sqlite3.connect(db) as con:
                con.execute("INSERT INTO identity_link VALUES (103, 203, 'VERIFIED')")
            second=m.identity_export(root,registry)
            self.assertNotEqual(first,second)
            self.assertEqual(first.read_bytes(),first_bytes)
            self.assertIn(b'103,203,VERIFIED',second.read_bytes())
            with sqlite3.connect(db) as con:
                con.execute("INSERT INTO identity_link VALUES (103, 204, 'VERIFIED')")
            with self.assertRaises(RuntimeError):
                m.identity_export(root,registry)

    def test_rebuild_requires_real_fide_pair_and_approved_receipts(self):
        with tempfile.TemporaryDirectory() as td:
            root=Path(td)
            inbox=root/"inbox"
            inbox.mkdir()
            (inbox/"unapproved.json").write_text(json.dumps({"approved_for_internal_use":False}))
            self.assertEqual(m.import_inbox(root,Path("/does/not/exist"),inbox)[0]["status"],"not-approved")
            self.assertEqual(m.build_month(root,Path("/does/not/exist"),"2026-11",Path("/no/identity"))["status"],"pending")

if __name__=="__main__":
    unittest.main()
