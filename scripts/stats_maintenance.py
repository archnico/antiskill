#!/usr/bin/env python3
"""Private, idempotent Antiskill Stats maintenance on archival coordinator n2."""
from __future__ import annotations
import argparse
import csv
import datetime as dt
import fcntl
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from zoneinfo import ZoneInfo

ROME = ZoneInfo("Europe/Rome")
KEYS = ("fsi-members-region-tos", "fsi-members-club-17029")
ROOT_DEFAULT = Path("/srv/archive/chess/antiskill")
BINARY_DEFAULT = Path("/home/nico/src/antiskill-stats-cli-20261119/target/release/antiskill")
INBOX_DEFAULT = Path("/home/nico/.local/state/antiskill-stats/fide-inbox")
IDENTITY_DEFAULT = Path("/srv/archive/chess/antiskill/durable/identity/verified-fsi-fide-2026-11-19.csv")
REGISTRY_DEFAULT = Path("/srv/archive/chess/identity-registry")
EXPORTER = Path("/home/nico/src/antiskill-stats-cli-20261119/scripts/export_verified_links.py")
STATUS_NAME = "stats-refresh-status.json"

def now():
    return dt.datetime.now(ROME)

def atomic_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile("w", dir=path.parent, prefix=".stats-", delete=False, encoding="utf-8") as handle:
        tmp=Path(handle.name)
        json.dump(value,handle,indent=2,sort_keys=True)
        handle.write("\n")
        handle.flush()
        os.fsync(handle.fileno())
    os.chmod(tmp,0o600)
    os.replace(tmp,path)

def capture(cmd):
    proc=subprocess.run([str(x) for x in cmd],capture_output=True,text=True,timeout=900,check=False)
    if proc.returncode:
        raise RuntimeError(f"Comando fallito ({proc.returncode}): {' '.join(str(x) for x in cmd[:3])}: {proc.stderr.strip()[-850:]}")
    return proc.stdout

def sha256(path):
    h=hashlib.sha256()
    with open(path,"rb") as handle:
        for chunk in iter(lambda:handle.read(1024*1024),b""):
            h.update(chunk)
    return h.hexdigest()

def previous_month(month):
    y,m=map(int,month.split("-"))
    return f"{y-1 if m==1 else y:04d}-{12 if m==1 else m-1:02d}"

def snapshot(root,key):
    ref=root/"raw"/"refs"/(key+".json")
    if not ref.exists(): return None
    j=json.loads(ref.read_text())
    if j.get("source_key")!=key:raise ValueError(f"Ref source_key incoerente: {key}")
    mpath=(root/j["manifest_path"]).resolve(strict=True)
    if not mpath.is_relative_to(root.resolve()):raise ValueError("Manifest fuori archivio")
    manifest=json.loads(mpath.read_text())
    raw=root/manifest["object_path"]
    path=raw.resolve(strict=True)
    if not path.is_relative_to((root/"raw"/"objects").resolve()):raise ValueError("Oggetto fuori archivio")
    digest=manifest.get("blake3","")
    if digest!=j.get("blake3") or len(digest)!=64 or digest!=path.name:raise ValueError(f"Ref/manifest incoerente: {key}")
    if path.stat().st_size!=manifest["bytes"]:raise ValueError(f"Dimensione oggetto incoerente: {key}")
    return {"source_key":key,"path":str(path),"manifest":str(mpath),"digest":digest,"timestamp_ms":manifest["fetched_at_unix_ms"],"rights":manifest.get("rights_class"),"url":manifest.get("source_url")}

def verify_digest(obj):
    actual=capture(["b3sum",obj["path"]]).split()[0]
    if actual!=obj["digest"]:raise ValueError(f"BLAKE3 incoerente per {obj['source_key']}")

def fsi_date(obj):
    return dt.datetime.fromtimestamp(obj["timestamp_ms"]/1000,ROME).date().isoformat()

def identity_export(root, registry_dir):
    candidates = sorted(registry_dir.glob("identity_registry_????-??-??.sqlite3"))
    if not candidates:
        raise ValueError("Registro identità corrente assente: impossibile aggiornare le associazioni FSI-FIDE")
    source = candidates[-1]
    source_hash = sha256(source)
    output = root/"durable"/"identity"/f"verified-fsi-fide-{source.stem[-10:]}-{source_hash[:16]}.csv"
    provenance = output.with_suffix(".csv.provenance.json")
    if not output.exists():
        if provenance.exists():raise ValueError("Export identità incompleto: manca CSV")
        output.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
        capture(["python3",EXPORTER,"--db",source,"--output",output])
    if not provenance.exists():raise ValueError("Export identità privo di provenienza")
    note=json.loads(provenance.read_text())
    if note.get("source_db_sha256")!=source_hash or note.get("crosswalk_sha256")!=sha256(output):
        raise ValueError("Il registro identità è cambiato durante l'export oppure CSV/provenienza divergenti")
    return output

def import_inbox(root,binary,inbox):
    results=[]
    if not inbox.is_dir():return results
    for receipt in sorted(inbox.glob("*.json")):
        entry=json.loads(receipt.read_text())
        if entry.get("approved_for_internal_use") is not True:
            results.append({"file":str(receipt),"status":"not-approved"});continue
        month=entry.get("month","");kind=entry.get("kind","")
        if len(month)!=7 or month[4]!="-":raise ValueError(f"Mese non valido: {receipt}")
        if kind not in ("combined","standard","rapid","blitz"):raise ValueError(f"Tipo FIDE non valido: {receipt}")
        file=(inbox/entry["file"]).resolve(strict=True)
        if not file.is_relative_to(inbox.resolve()) or file.suffix.lower()!=".zip":raise ValueError("ZIP esterno all'inbox")
        digest=sha256(file)
        if entry.get("sha256")!=digest:raise ValueError(f"SHA256 mancante/discordante: {file}")
        key=f"fide-rating-{kind}-{month}"
        existing=snapshot(root,key)
        if existing:
            staged_digest=capture(["b3sum",file]).split()[0]
            if staged_digest!=existing["digest"]:
                raise ValueError(f"FIDE {key} differs from immutable imported release; manual reconciliation required")
            results.append({"file":str(file),"status":"already-imported","source":key});continue
        capture([binary,"sources","import","fide-local","--root",root,"--file",file,"--month",month,"--kind",kind,"--url",entry["url"]])
        new=snapshot(root,key)
        if not new:raise ValueError("Import FIDE non persistito")
        verify_digest(new)
        results.append({"file":str(file),"status":"imported","source":key})
    return results

def latest_fide(root):
    keys=list((root/"raw"/"refs").glob("fide-rating-combined-????-??.json"))
    return sorted(p.stem.removeprefix("fide-rating-combined-") for p in keys)[-1] if keys else None

def scoped_identity_digest(identity, rosters):
    """Hash only relevant FSI identities, including unverified/conflicting assertions."""
    members = set()
    for obj in rosters:
        with open(obj["path"], encoding="utf-8-sig", newline="") as handle:
            for row in csv.reader(handle, delimiter=";"):
                if len(row) != 15:
                    raise ValueError("Roster FSI non valido durante fingerprint")
                members.add(row[0])
    scoped = []
    seen = set()
    with open(identity, encoding="utf-8", newline="") as handle:
        rd = csv.DictReader(handle)
        if rd.fieldnames != ["fsi_id", "fide_id", "status"]:
            raise ValueError("Intestazione crosswalk identità incompatibile")
        for row in rd:
            fsi = row["fsi_id"]
            if fsi in members:
                if fsi in seen:
                    raise ValueError("FSI duplicato nel crosswalk identità")
                if row["status"] not in ("VERIFIED", "REVIEW", "SOURCE_ASSERTED"):
                    raise ValueError("Stato crosswalk sconosciuto")
                seen.add(fsi)
                scoped.append((fsi, row["fide_id"], row["status"]))
    scoped.sort()
    return hashlib.sha256(json.dumps(scoped, separators=(",", ":")).encode()).hexdigest()


def already_promoted_same_scope(root, link, objects, identity, roster_scope_hash):
    if not link.is_symlink():
        return False
    target = link.resolve(strict=True)
    if not target.is_relative_to((root/"reports"/"versions").resolve()):
        raise ValueError("Link managed inatteso")
    meta = json.loads((target/"run-meta.json").read_text())
    current_sources = {o["source_key"]:o["digest"] for o in objects}
    if meta.get("sources") != current_sources:
        return False
    if meta.get("identity_scope_sha256"):
        return meta["identity_scope_sha256"] == roster_scope_hash
    old_sha = meta.get("identity_sha256")
    if old_sha == sha256(identity):
        return True
    # Older output predates the semantic fingerprint; resolve its original
    # private crosswalk rather than assuming the new national export is equal.
    old_matches = [p for p in (root/"durable"/"identity").glob("verified-fsi-fide-*.csv")
                   if sha256(p) == old_sha]
    if len(old_matches) != 1:
        return False
    rosters = [o for o in objects if o["source_key"].startswith("fsi-members-")]
    return scoped_identity_digest(old_matches[0], rosters) == roster_scope_hash


def build_month(root,binary,month,identity):
    current=snapshot(root,f"fide-rating-combined-{month}")
    prev_month=previous_month(month)
    previous={}
    for rating in ("standard","rapid"):
        combined=snapshot(root,f"fide-rating-combined-{prev_month}")
        historical=snapshot(root,f"fide-rating-{rating}-{prev_month}")
        if combined:
            previous[rating]=(combined,"combined")
        elif historical:
            previous[rating]=(historical,rating)
    if not current:return {"status":"pending","reason":f"Lista FIDE combinata {month} non presente"}
    if not all(r in previous for r in ("standard","rapid")):
        return {"status":"pending","reason":f"Lista FIDE {prev_month} Standard/Rapid incompleta: delta non calcolabile"}
    region=snapshot(root,f"fsi-members-region-tos-{month[:4]}")
    club=snapshot(root,f"fsi-members-club-17029-{month[:4]}")
    allin=snapshot(root,"torneionline-allin")
    if not (region and club and allin):return {"status":"pending","reason":"Manca FSI Toscana/17029 o Allin"}
    if fsi_date(region)[:7]!=month or fsi_date(club)[:7]!=month:
        return {"status":"pending","reason":f"FSI as-of non coincide con mese FIDE {month}"}
    if not identity.is_file():return {"status":"pending","reason":"Registro FSI-FIDE VERIFIED esportato non presente"}
    objs=[current,region,club,allin]+list({v[0]["digest"]:v[0] for v in previous.values()}.values())
    for obj in objs:verify_digest(obj)
    scoped_hash=scoped_identity_digest(identity,[region,club])
    fingerprint=hashlib.sha256(("|".join(o["digest"] for o in objs)+"|"+scoped_hash).encode()).hexdigest()
    link=root/"reports"/f"rankings-{month}-managed"
    if already_promoted_same_scope(root,link,objs,identity,scoped_hash):
        return {"status":"unchanged","month":month,"reason":"Fonte/identita FSI rilevanti invariate","scope_identity_sha256":scoped_hash}
    version=(root/"reports"/"versions"/month/(fingerprint[:16]+"-"+now().strftime("%Y%m%dT%H%M%S"))).resolve()
    if version.exists():raise ValueError("Versione report già presente: rifiuto overwrite")
    version.mkdir(parents=True,mode=0o700)
    outputs=[
        ("top3-tos-standard.json","top3",region,["--region","TOS","--rating","standard"]),
        ("club-17029-standard.json","club",club,["--club","17029","--rating","standard"]),
        ("club-17029-rapid-u18.json","club",club,["--club","17029","--rating","rapid","--under","18"]),
        ("support/top3-tos-standard-full.json","top3",region,
         ["--region","TOS","--rating","standard","--limit","100000"]),
    ]
    for filename,mode,roster,other in outputs:
        output=version/filename
        output.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
        rating=other[other.index("--rating")+1]
        prior,prior_kind=previous[rating]
        capture([binary,"rankings",mode,"--snapshot",allin["path"],"--month",month,
            "--fsi-csv",roster["path"],"--fsi-manifest",roster["manifest"],"--fsi-as-of",fsi_date(roster),
            "--verified-links",identity,"--fide-xml",current["path"],"--fide-month",month,
            "--previous-fide-xml",prior["path"],"--previous-fide-month",prev_month,
            "--previous-fide-kind",prior_kind,"--require-official","--output",output,*other])
        doc=json.loads(output.read_text())
        if doc.get("schema")!="antiskill.rankings.v1" or doc.get("period")!=month or doc.get("certified") is not False or doc.get("previous_fide_period")!=previous_month(month):
            raise ValueError(f"Report non valido: {filename}")
        if doc.get("fsi_source",{}).get("blake3")!=roster["digest"] or doc.get("fide_source",{}).get("blake3")!=current["digest"]:
            raise ValueError(f"Provenienza report incoerente: {filename}")
        if filename.startswith("support/"):
            for group in doc.get("groups",[]):
                if len(group.get("players",[]))!=group.get("population"):
                    raise ValueError(f"Snapshot classifica completa troncato: {filename}")
    full_now=version/"support/top3-tos-standard-full.json"
    full_old=root/"reports"/f"rankings-{prev_month}-managed"/"support/top3-tos-standard-full.json"
    if not full_old.is_file():
        full_old=root/"reports"/"full-snapshots"/prev_month/"top3-tos-standard-full.json"
    movement={"status":"pending","reason":"Snapshot regionale integrale mese precedente non disponibile"}
    if full_old.is_file():
        try:
            result=capture([binary,"stats","movement","--current",full_now,"--previous",full_old,
                            "--cohort","U16","--sex","absolute","--format","json"])
            report=json.loads(result)
            if report.get("meta",{}).get("certified") is not False:
                raise ValueError("Movimenti non certificati: flag errato")
            atomic_json(version/"support/movement-u16-absolute.json",report)
            movement={"status":"computed","rows":len(report.get("rows",[]))}
        except (RuntimeError, ValueError) as e:
            movement={"status":"pending","reason":str(e)}
    atomic_json(version/"run-meta.json",{"schema":"antiskill.stats-managed.v1","fingerprint":fingerprint,"period":month,
        "created_at":now().isoformat(),"identity_sha256":sha256(identity),
        "identity_scope_sha256":scoped_hash,"sources":{o["source_key"]:o["digest"] for o in objs},
        "reports":[entry[0] for entry in outputs],"movement":movement,"published":False,"certified":False})
    # Only after ALL reports validate do we promote the new complete version.
    tmp_link=root/"reports"/f".rankings-{month}-managed-{os.getpid()}"
    os.symlink(version,tmp_link)
    os.replace(tmp_link,link)
    return {"status":"built","month":month,"fingerprint":fingerprint,"reports":[entry[0] for entry in outputs]}

def report_status(root):
    month=latest_fide(root)
    wanted=now().strftime("%Y-%m")
    sources={}
    for key in [f"fsi-members-region-tos-{now().year}",f"fsi-members-club-17029-{now().year}",
                f"fide-rating-combined-{wanted}",f"fide-rating-combined-{previous_month(wanted)}"]:
        obj=snapshot(root,key)
        sources[key]={"as_of":fsi_date(obj),"blake3":obj["digest"],"source_url":obj["url"]} if obj else None
    return {"current_month":wanted,"latest_fide_combined_month":month,"sources":sources,
            "rating_current_month_available":month==wanted,"certificate":"not-certified"}

def main(argv=None):
    ap=argparse.ArgumentParser(description="Antiskill private stats maintenance: sync FSI, import approved local FIDE ZIP, rebuild immutable reports")
    ap.add_argument("action",choices=("status","daily","build","import"))
    ap.add_argument("--root",type=Path,default=ROOT_DEFAULT)
    ap.add_argument("--bin",type=Path,default=BINARY_DEFAULT)
    ap.add_argument("--inbox",type=Path,default=INBOX_DEFAULT)
    ap.add_argument("--identity",type=Path,help="Override manuale per CSV VERIFIED (altrimenti export del registro attuale)")
    ap.add_argument("--registry-dir",type=Path,default=REGISTRY_DEFAULT)
    ap.add_argument("--month",help="YYYY-MM (per build)")
    opts=ap.parse_args(argv)
    root=opts.root
    status_path=root/"reports"/STATUS_NAME
    if opts.action=="status":
        result=report_status(root)
        if status_path.exists():result["last_run"]=json.loads(status_path.read_text())
        print(json.dumps(result,indent=2))
        return 0
    state={"action":opts.action,"started_at":now().isoformat(),"certified":False,"published":False}
    lock=root/"reports"/".stats-refresh.lock"
    lock.parent.mkdir(parents=True,exist_ok=True)
    with open(lock,"a+") as lockfile:
        try:fcntl.flock(lockfile.fileno(),fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:raise RuntimeError("Un refresh Stats è già in corso")
        try:
            if opts.action=="daily":
                state["fsi_updates"]=[]
                for scope in (["--region","TOS"],["--club","17029"]):
                    stdout=capture([opts.bin,"sources","sync","fsi-members","--root",root,"--year",now().year,*scope])
                    state["fsi_updates"].append({"scope":scope[-1],"result":stdout.splitlines()[:2]})
            if opts.action in ("daily","import"):
                state["fide_imports"]=import_inbox(root,opts.bin,opts.inbox)
            if opts.action in ("daily","build"):
                month=opts.month or now().strftime("%Y-%m")
                if month>now().strftime("%Y-%m"):raise ValueError("Mese futuro non consentito")
                identity=opts.identity or identity_export(root,opts.registry_dir)
                state["identity_export"]=str(identity)
                state["build"]=build_month(root,opts.bin,month,identity)
            state["result"]="ok"
        except Exception as exc:
            state["result"]="error";state["error"]=str(exc)
            atomic_json(status_path,{**state,"finished_at":now().isoformat()})
            raise
        atomic_json(status_path,{**state,"finished_at":now().isoformat()})
        print(json.dumps(state,indent=2))
    return 0

if __name__=="__main__":
    try:sys.exit(main())
    except Exception as exc:
        print("antiskill-stats-maintenance: "+str(exc),file=sys.stderr)
        sys.exit(2)
