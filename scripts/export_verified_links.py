#!/usr/bin/env python3
"""Read-only, privacy-preserving FSI↔FIDE identity crosswalk exporter.

Only identifiers and their registry verification status are exported.
Never publish these raw data; output permissions are restrictive.
"""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import sqlite3
from datetime import datetime, timezone


def digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def export(db_path, output):
    db_path = Path(db_path).resolve(strict=True)
    output = Path(output)
    provenance = output.with_suffix(output.suffix + ".provenance.json")
    if output.exists() or provenance.exists():
        raise RuntimeError("Output already exists: no overwrite permitted")
    if not output.parent.exists():
        raise RuntimeError("Create the output directory explicitly, mode 0700")
    db = sqlite3.connect(db_path.as_uri() + "?mode=ro", uri=True)
    try:
        fields = {r[1] for r in db.execute("PRAGMA table_info(identity_link)")}
        if not {"fsi_id", "fide_id", "status"} <= fields:
            raise RuntimeError("identity_link schema incomplete")
        duplicates = db.execute(
            "SELECT COUNT(*) FROM "
            "(SELECT fsi_id FROM identity_link GROUP BY fsi_id HAVING COUNT(*)>1)"
        ).fetchone()[0]
        if duplicates:
            raise RuntimeError("Duplicate FSI ids: requires identity review")
        verified_duplicate_fide = db.execute(
            "SELECT COUNT(*) FROM "
            "(SELECT fide_id FROM identity_link WHERE status='VERIFIED' "
            "GROUP BY fide_id HAVING COUNT(*)>1)"
        ).fetchone()[0]
        if verified_duplicate_fide:
            raise RuntimeError("VERIFIED FIDE id mapped to multiple FSI ids")
        os.umask(0o077)
        counts = {"VERIFIED": 0, "REVIEW": 0, "SOURCE_ASSERTED": 0}
        with open(output, "x", newline="", encoding="utf-8") as f:
            writer = csv.writer(f)
            writer.writerow(["fsi_id", "fide_id", "status"])
            for fsi, fide, state in db.execute(
                "SELECT fsi_id, fide_id, status FROM identity_link ORDER BY fsi_id"
            ):
                if state not in counts:
                    raise RuntimeError(f"Unexpected identity status: {state}")
                if not isinstance(fsi, int) or not isinstance(fide, int):
                    raise RuntimeError("Invalid numeric identity")
                writer.writerow([fsi, fide, state])
                counts[state] += 1
            f.flush()
            os.fsync(f.fileno())
        meta = {
            "schema": "antiskill.identity-crosswalk-export.v1",
            "source_db_sha256": digest(db_path),
            "crosswalk_sha256": digest(output),
            "exported_at": datetime.now(timezone.utc).isoformat(),
            "source_table": "identity_link",
            "status_counts": counts,
            "records": sum(counts.values()),
            "rights_class": "internal",
            "publication_allowed": False,
        }
        with open(provenance, "x", encoding="utf-8") as f:
            json.dump(meta, f, sort_keys=True, indent=2)
            f.write("\n")
        os.chmod(output, 0o600)
        os.chmod(provenance, 0o600)
        print(json.dumps({"records": meta["records"], "counts": counts,
                          "crosswalk_sha256": meta["crosswalk_sha256"]}))
    except BaseException:
        if output.exists() and not provenance.exists():
            output.unlink()
        raise
    finally:
        db.close()


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--db", required=True, help="national identity registry SQLite")
    ap.add_argument("--output", required=True, help="new private CSV path; no overwrite")
    args = ap.parse_args()
    export(args.db, args.output)
