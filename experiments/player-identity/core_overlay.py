#!/usr/bin/env python3
"""Read-only federation identity overlay for existing Antiskill PGN collection.

Delegates all occurrence IDs, identity and PGN semantics to Antiskill's own
`players --format json` command. This tool never parses or rewrites a PGN,
deduplicates a game, or promotes unverified FSI/FIDE pairs.
"""
import argparse
import json
import re
import sqlite3
import subprocess
from pathlib import Path

HEX64=re.compile(r"^[0-9a-f]{64}$")
GAME=re.compile(r"^g1-[0-9a-f]{64}$")

def validate_core_view(view, fide_id):
    if not isinstance(view,dict):
        raise ValueError("Expected object from Antiskill")
    kind=view.get("profile")
    if kind not in (
        "antiskill_player_identity_view_v1",
        "antiskill_player_identity_view_v2",
        "antiskill_player_identity_view_v3"):
        raise ValueError("Unknown identity schema: "+str(kind))
    if str(view.get("fide_id"))!=str(fide_id):
        raise ValueError("Antiskill player ID differs from requested ID")
    digest=view.get("collection_blake3")
    if not isinstance(digest,str) or not HEX64.fullmatch(digest):
        raise ValueError("Invalid core collection digest")
    if kind.endswith("_v1"):
        occurrences=view.get("claims")
    else:
        occurrences=view.get("verified_occurrences")
    if not isinstance(occurrences,list):
        raise ValueError("Missing core authoritative occurrences")
    found=set()
    for row in occurrences:
        if not isinstance(row,dict) or not isinstance(row.get("occurrence_id"),str):
            raise ValueError("Invalid occurrence")
        gid=row["occurrence_id"]
        if not GAME.fullmatch(gid):
            raise ValueError("Invalid core occurrence identifier")
        if gid in found:
            raise ValueError("Duplicate occurrence IDs in core response")
        found.add(gid)
    return kind, occurrences

def assemble(db,view,fide_id):
    kind, occurrences=validate_core_view(view,fide_id)
    db.row_factory=sqlite3.Row
    links=[dict(r) for r in db.execute(
        "SELECT fsi_id,fide_id,status,reason,source_key FROM identity_link WHERE fide_id=?",
        (fide_id,))]
    verified=[r for r in links if r["status"]=="VERIFIED"]
    conflicting=[r for r in links if r["status"]!="VERIFIED"]
    if len(verified)>1:
        raise ValueError("Multiple verified FSI IDs for one FIDE ID; refuse attribution")
    # Never infer an FSI ID from name. Disputed mappings are listed as evidence,
    # but the exported person attribution stays unlinked.
    if conflicting:
        fsi_attribution=None
        relation="REVIEW_REQUIRED"
    elif len(verified)==1:
        fsi_attribution=verified[0]["fsi_id"]
        relation="VERIFIED_ID_CROSSWALK"
    else:
        fsi_attribution=None
        relation="NO_VERIFIED_FSI_LINK"
    if kind.endswith("_v2") or kind.endswith("_v3"):
        ext=[r for r in view.get("external_ids",[]) if isinstance(r,dict) and r.get("namespace")=="fsi"]
        if ext and (fsi_attribution is None or any(str(r.get("value"))!=str(fsi_attribution) for r in ext)):
            raise ValueError("Native FSI bridge disagrees with official current crosswalk")
    return {
        "profile":"antiskill_player_identity_overlay_v1",
        "fide_id":str(fide_id),
        "fsi_id":fsi_attribution,
        "fsi_relation":relation,
        "fsi_crosswalk_evidence":links,
        "core_profile":kind,
        "core_entity_id":view.get("player_entity_id"),
        "core_collection_blake3":view["collection_blake3"],
        "core_occurrences":occurrences,
        "core_occurrence_count":len(occurrences),
        "core_conflicts":view.get("conflicts",[]),
        "core_unresolved_candidates":view.get("unresolved_candidates",view.get("candidates",[])),
        "warning":"No game deduplication or identity merge is performed. Candidate accounts and candidate PGN aliases are excluded from authoritative attribution."
    }

def run(workspace,binpath,dbpath,fide_id,report,bridge=None,context=None):
    if not workspace.is_dir():
        raise ValueError("Active Antiskill workspace directory does not exist")
    if fide_id<=0:
        raise ValueError("Invalid FIDE ID")
    args=[str(binpath),"players","--fide-id",str(fide_id),"--format","json"]
    if bridge:
        args.extend(["--federation-index",str(bridge.resolve())])
    if context:
        args.extend(["--context-registry",str(context.resolve())])
    completed=subprocess.run(args,cwd=workspace,capture_output=True,text=True,
                             timeout=300,check=False)
    if completed.returncode:
        raise RuntimeError("Antiskill players failed: "+completed.stderr.strip()[:600])
    view=json.loads(completed.stdout)
    with sqlite3.connect(f"file:{dbpath.resolve()}?mode=ro",uri=True) as db:
        overlay=assemble(db,view,fide_id)
    report.write_text(json.dumps(overlay,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    return overlay

if __name__=="__main__":
    p=argparse.ArgumentParser()
    p.add_argument("--db",type=Path,required=True)
    p.add_argument("--workspace",type=Path,required=True)
    p.add_argument("--antiskill-bin",type=Path,required=True)
    p.add_argument("--fide-id",type=int,required=True)
    p.add_argument("--federation-index",type=Path)
    p.add_argument("--context-registry",type=Path)
    p.add_argument("--report",type=Path,required=True)
    a=p.parse_args()
    overlay=run(a.workspace,a.antiskill_bin,a.db,a.fide_id,a.report,
                a.federation_index,a.context_registry)
    print(json.dumps({"profile":overlay["profile"],
                      "fsi_relation":overlay["fsi_relation"],
                      "core_occurrence_count":overlay["core_occurrence_count"],
                      "report":str(a.report)},ensure_ascii=False,indent=2))
