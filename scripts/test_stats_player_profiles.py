#!/usr/bin/env python3
import importlib.util
import io
import json
import sqlite3
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

DIR=Path(__file__).parent
sys.path.insert(0,str(DIR))
spec=importlib.util.spec_from_file_location("player_profiles", DIR/"build_stats_player_profiles.py")
profiles=importlib.util.module_from_spec(spec)
spec.loader.exec_module(profiles)

class PlayerProfileTests(unittest.TestCase):
    def test_combined_fide_xml_three_ratings_and_birth_year_conflict(self):
        with tempfile.TemporaryDirectory() as td:
            root=Path(td)
            xml=("""<playerslist>
              <player><fideid>28101</fideid><name>Rossi, Mario</name><country>ITA</country>
              <title>FM</title><rating>1843</rating><rapid_rating>0</rapid_rating>
              <blitz_rating>1710</blitz_rating><birthday>1990</birthday></player>
              <player><fideid>28102</fideid><name>Bianchi, Anna</name><country>FRA</country>
              <title></title><w_title>WFM</w_title><rating>2010</rating>
              <rapid_rating>1940</rapid_rating><blitz_rating>1870</blitz_rating>
              <birthday>2004</birthday></player></playerslist>""")
            z=root/"fide.zip"
            with zipfile.ZipFile(z,"w",compression=zipfile.ZIP_DEFLATED) as archive:
                archive.writestr("fide.xml",xml)
            players={
                "100":{"fsi_id":"100","fide_id":"28101","birth_year":1990,"fide_title":None,
                       "fide_standard":None,"fide_rapid":None,"fide_blitz":None,
                       "fide_rating_status":"not_in_monthly_list","fide_federation":None,
                       "full_name":"ROSSI MARIO","surname":None,"given_name":None},
                "101":{"fsi_id":"101","fide_id":"28102","birth_year":2001,"fide_title":None,
                       "fide_standard":None,"fide_rapid":None,"fide_blitz":None,
                       "fide_rating_status":"not_in_monthly_list","fide_federation":None,
                       "full_name":None,"surname":None,"given_name":None}
            }
            stats=profiles.scan_fide(z,players,min_seen=2)
            self.assertEqual(stats["xml_players_seen"],2)
            self.assertEqual(stats["linked_fide_players_found"],2)
            first=players["100"]
            self.assertEqual(first["fide_standard"],1843)
            self.assertIsNone(first["fide_rapid"])
            self.assertEqual(first["fide_blitz"],1710)
            self.assertEqual(first["fide_federation"],"ITA")
            self.assertEqual(first["fide_title"],"FM")
            self.assertEqual(first["surname"],"Rossi")
            self.assertEqual(first["given_name"],"Mario")
            second=players["101"]
            self.assertEqual(second["fide_rating_status"],"birth_year_conflict")
            self.assertIsNone(second["fide_standard"])
            self.assertEqual(second["fide_birth_year"],2004)
            self.assertEqual(second["fide_title"],"WFM")
            self.assertIsNone(second["birth_date"] if "birth_date" in second else None)
    def test_scoped_fsi_names_category_and_nationality_are_not_fide_federation(self):
        players={
            "100":{"birth_year":1990,"full_name":"Rossi, Mario","surname":None,"given_name":None,
                   "nationality_code":None,"fsi_category":"CM",
                   "fsi_category_source":"registry:allin","fsi_official_as_of":None},
        }
        officials={"100":{"surname":"ROSSI","given_name":"MARIO","birth_year":1990,
                           "fsi_category":"1N","nationality_code":"ITA",
                           "club":"17029","fsi_name_source":"fsi-members-region-tos-2026"}}
        profiles.apply_fsi(players,officials,2026)
        p=players["100"]
        self.assertEqual(p["surname"],"ROSSI")
        self.assertEqual(p["given_name"],"MARIO")
        self.assertEqual(p["fsi_category"],"1N")
        self.assertEqual(p["nationality_code"],"ITA")
        self.assertEqual(p["fsi_category_source"],"fsi-members-region-tos-2026")
        self.assertNotIn("birth_year_conflict",p)
    def test_registry_only_verified_links_and_source_provenance(self):
        with tempfile.TemporaryDirectory() as td:
            r=Path(td)
            db=r/"registry.sqlite3"
            with sqlite3.connect(db) as con:
                con.execute("CREATE TABLE identity_link(fsi_id INTEGER,fide_id INTEGER,status TEXT)")
                con.execute("CREATE TABLE fsi_player(fsi_id INTEGER,full_name TEXT,birth_year INTEGER,category TEXT,federation TEXT,source_key TEXT)")
                con.execute("CREATE TABLE fide_player(fide_id INTEGER,full_name TEXT,birth_year INTEGER,fed TEXT,title TEXT,source_key TEXT)")
                con.executemany("INSERT INTO identity_link VALUES(?,?,?)",
                    [(100,28101,"VERIFIED"),(101,28102,"REVIEW")])
                con.execute("INSERT INTO fsi_player VALUES(100,'ROSSI MARIO',1990,'1N','ITA','allin-2026-04')")
                con.execute("INSERT INTO fide_player VALUES(28101,'Rossi, Mario',1990,'ITA','FM','fide-current')")
            cross=r/"links.csv"
            cross.write_text("fsi_id,fide_id,status\n100,28101,VERIFIED\n101,28102,REVIEW\n")
            p=profiles.load_registry(db,cross)
            self.assertEqual(list(p),["100"])
            self.assertEqual(p["100"]["fide_title"],"FM")
            self.assertIsNone(p["100"]["nationality_code"])

if __name__=="__main__":
    unittest.main()
