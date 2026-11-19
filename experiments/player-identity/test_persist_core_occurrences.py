import copy
import sqlite3
import tempfile
import unittest
from pathlib import Path
from persist_core_occurrences import materialize

def overlay():
    return {
       "profile":"antiskill_player_identity_overlay_v1",
       "core_profile":"antiskill_player_identity_view_v1",
       "fsi_relation":"VERIFIED_ID_CROSSWALK",
       "fsi_id":12345,"fide_id":"98765432",
       "fsi_crosswalk_evidence":[{"fsi_id":12345,"fide_id":98765432,
                                  "status":"VERIFIED","source_key":"mock-allin"}],
       "core_collection_blake3":"a"*64,
       "core_occurrence_count":2,
       "core_occurrences":[
         {"occurrence_id":"g1-"+"b"*64,"side":"white",
          "source_blake3":"c"*64,"source_bytes":1000,"source_start":3,
          "source_end":140,"ordinal":1},
         {"occurrence_id":"g1-"+"d"*64,"side":"black",
          "source_blake3":"c"*64,"source_bytes":1000,"source_start":141,
          "source_end":290,"ordinal":2}
       ]
    }

class PersistCoreTests(unittest.TestCase):
    def test_roundtrip_idempotent_and_no_game_merge(self):
        with tempfile.TemporaryDirectory() as root:
            con=sqlite3.connect(Path(root)/"synthetic.sqlite3")
            con.execute("CREATE TABLE source(source_key TEXT PRIMARY KEY,sha256 TEXT)")
            con.execute("INSERT INTO source VALUES ('mock-allin',?)",("f"*64,))
            con.execute("CREATE TABLE identity_link(fsi_id INTEGER,fide_id INTEGER,status TEXT,source_key TEXT)")
            con.execute("INSERT INTO identity_link VALUES(12345,98765432,'VERIFIED','mock-allin')")
            con.commit()
            v=overlay()
            a=materialize(con,v)
            self.assertEqual((a["requested"],a["inserted"]),(2,2))
            b=materialize(con,v)
            self.assertEqual(b["inserted"],0)
            self.assertEqual(con.execute("SELECT count(*) FROM verified_core_player_occurrence").fetchone()[0],2)
            # Same game moves are not a player-level game identity: different
            # occurrence IDs remain separate even with identical source hash.
            self.assertEqual(con.execute("SELECT count(DISTINCT occurrence_id) FROM verified_core_player_occurrence").fetchone()[0],2)
            v["core_occurrences"][1]["source_start"]=160
            with self.assertRaisesRegex(ValueError,"Contradictory"):
                materialize(con,v)
            self.assertEqual(con.execute("SELECT min(source_start) FROM verified_core_player_occurrence").fetchone()[0],3)
            self.assertEqual(con.execute("PRAGMA integrity_check").fetchone()[0],"ok")
            con.close()

    def test_conflicting_crosswalk_unverified_and_bad_source_are_blocked(self):
        with tempfile.TemporaryDirectory() as root:
            con=sqlite3.connect(Path(root)/"synthetic.sqlite3")
            con.execute("CREATE TABLE source(source_key TEXT PRIMARY KEY,sha256 TEXT)")
            con.execute("INSERT INTO source VALUES ('mock-allin',?)",("f"*64,))
            con.execute("CREATE TABLE identity_link(fsi_id INTEGER,fide_id INTEGER,status TEXT,source_key TEXT)")
            con.execute("INSERT INTO identity_link VALUES(12345,98765432,'VERIFIED','mock-allin')")
            con.commit()
            v=overlay()
            v["fsi_relation"]="REVIEW_REQUIRED"
            with self.assertRaisesRegex(ValueError,"unresolved"):
                materialize(con,v)
            v=overlay()
            v["core_occurrences"][0]["source_blake3"]="not-a-hash"
            with self.assertRaisesRegex(ValueError,"source BLAKE3"):
                materialize(con,v)
            v=overlay()
            con.execute("INSERT INTO identity_link VALUES(12345,321,'REVIEW','mock-allin')")
            with self.assertRaisesRegex(ValueError,"conflicting"):
                materialize(con,v)
            con.close()

if __name__=="__main__":
    unittest.main()
