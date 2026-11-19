#!/usr/bin/env python3
"""Read-only, evidence-scoped FSI/FIDE discrepancy audit.

This is an audit VIEW of the existing Player Identity Italia SQLite pilot, not
another identity store and not a promotion mechanism. It never modifies the DB,
never resolves a name-only identity, and excludes names/birth dates from output.
"""
from __future__ import annotations

import argparse
import collections
from contextlib import contextmanager
import json
import re
import sqlite3
from pathlib import Path
from urllib.parse import quote

from ingest import canonical_name

PROFILE = "antiskill_identity_source_discrepancy_v1"
SHA256 = re.compile(r"^[0-9a-f]{64}$")
REQUIRED = {
    "source": {"source_key", "sha256", "source_role"},
    "fsi_fide_assertion": {"fsi_id", "fide_id", "source_key"},
    "identity_link": {"fsi_id", "fide_id", "status", "reason", "source_key"},
    "fsi_player": {"fsi_id", "full_name", "birth_year", "source_key"},
    "fide_player": {"fide_id", "full_name", "birth_year", "source_key"},
    "fsi_official_name_observation": {
        "fsi_id", "full_name", "birth_year", "source_key", "source_sha256"
    },
    "external_id_pair_observation": {"fsi_id", "fide_id", "source_key", "source_sha256"},
    "externally_certified_crosswalk": {
        "fsi_id", "fide_id", "tournament_source_key",
        "tournament_source_sha256", "official_fsi_source_key",
        "official_fsi_source_sha256", "official_fide_source_key",
        "official_fide_source_sha256", "certification_rule"
    },
    "crosswalk_candidate": {"fsi_id", "fide_id", "status"},
}


def name_signature(raw):
    return " ".join(sorted(canonical_name(raw or "").split()))


def verified_fingerprint(value, label):
    if not isinstance(value, str) or not SHA256.fullmatch(value):
        raise ValueError(f"Invalid SHA256 for {label}")
    return value


@contextmanager
def connect_readonly(path: Path):
    if not path.is_file() or path.is_symlink():
        raise ValueError("Input must be a regular, non-symlink SQLite file")
    uri = "file:" + quote(str(path.absolute()), safe="/") + "?mode=ro"
    db = sqlite3.connect(uri, uri=True)
    try:
        db.row_factory = sqlite3.Row
        db.execute("PRAGMA query_only=ON")
        tables = {row[0] for row in db.execute(
            "SELECT name FROM sqlite_master WHERE type='table'"
        )}
        for table, required in REQUIRED.items():
            if table not in tables:
                raise ValueError(f"Required table missing: {table}")
            columns = {row[1] for row in db.execute(f"PRAGMA table_info({table})")}
            if not required <= columns:
                raise ValueError(f"Required columns missing: {table}")
        # Freeze a consistent read view even if another agent adds observations.
        db.execute("BEGIN")
        yield db
    finally:
        db.close()


def index_source(db):
    sources = {}
    for row in db.execute("SELECT source_key,sha256,source_role FROM source"):
        key = row["source_key"]
        if key in sources:
            raise ValueError(f"Duplicated source key: {key}")
        sources[key] = {
            "sha256": verified_fingerprint(row["sha256"], key),
            "role": row["source_role"],
        }
    for key in ("fide-current", "allin-2026-04"):
        if key not in sources:
            raise ValueError(f"Missing required source snapshot: {key}")
    return sources


def source_hash(sources, key):
    if key not in sources:
        raise ValueError(f"Source key has no manifest: {key}")
    return sources[key]["sha256"]


def indexed_rows(db, table, key):
    out = collections.defaultdict(list)
    for row in db.execute(f"SELECT * FROM {table}"):
        out[row[key]].append(row)
    return out


def certificate_errors(cert, sources, link):
    errors = []
    proof_pairs = (
        ("tournament_source_key", "tournament_source_sha256"),
        ("official_fsi_source_key", "official_fsi_source_sha256"),
        ("official_fide_source_key", "official_fide_source_sha256"),
    )
    keys = []
    for key_field, hash_field in proof_pairs:
        key, digest = cert[key_field], cert[hash_field]
        keys.append(key)
        if key not in sources:
            errors.append("CERTIFICATE_SOURCE_MISSING")
        elif not SHA256.fullmatch(str(digest or "")):
            errors.append("CERTIFICATE_HASH_MALFORMED")
        elif sources[key]["sha256"] != digest:
            errors.append("CERTIFICATE_SOURCE_HASH_CHANGED")
    if len(set(keys)) != 3:
        errors.append("CERTIFICATE_SOURCES_NOT_DISTINCT")
    if cert["certification_rule"] != "manual_selection_three_source_exact_id_name_birthyear_v1":
        errors.append("CERTIFICATE_RULE_UNRECOGNIZED")
    for expected, value in zip(("chess-results-", "fsi-official-tesserato-", "fide-current"), keys):
        if not value.startswith(expected):
            errors.append("CERTIFICATE_SOURCE_ROLE_MISMATCH")
    if link is None or link["status"] != "VERIFIED":
        errors.append("CERTIFICATE_LINK_NOT_VERIFIED")
    return sorted(set(errors))


def audit(db):
    """Return deterministic aggregate report and privacy-minimized discrepancy rows."""
    sources = index_source(db)
    assertions = db.execute("""
        SELECT a.fsi_id,a.fide_id,a.source_key,
               l.status AS link_status,l.reason AS link_reason,
               fp.full_name AS fsi_name,fp.birth_year AS fsi_birth_year,
               fp.source_key AS fsi_source,
               fd.full_name AS fide_name,fd.birth_year AS fide_birth_year,
               fd.source_key AS fide_source
        FROM fsi_fide_assertion a
        LEFT JOIN identity_link l ON l.fsi_id=a.fsi_id AND l.fide_id=a.fide_id
        LEFT JOIN fsi_player fp ON fp.fsi_id=a.fsi_id
        LEFT JOIN fide_player fd ON fd.fide_id=a.fide_id
        ORDER BY a.fsi_id,a.fide_id
    """).fetchall()
    if not assertions:
        raise ValueError("No source ID assertions: cannot infer a zero-discrepancy result")

    by_fsi = collections.defaultdict(set)
    by_fide = collections.defaultdict(set)
    for r in assertions:
        if not isinstance(r["fsi_id"], int) or r["fsi_id"] <= 0:
            raise ValueError("Invalid FSI ID in assertion")
        if not isinstance(r["fide_id"], int) or r["fide_id"] <= 0:
            raise ValueError("Invalid FIDE ID in assertion")
        by_fsi[r["fsi_id"]].add(r["fide_id"])
        by_fide[r["fide_id"]].add(r["fsi_id"])
        source_hash(sources, r["source_key"])
        if r["fsi_source"] is not None:
            source_hash(sources, r["fsi_source"])
        if r["fide_source"] is not None:
            source_hash(sources, r["fide_source"])

    official = indexed_rows(db, "fsi_official_name_observation", "fsi_id")
    external = db.execute(
        "SELECT * FROM external_id_pair_observation ORDER BY fsi_id,fide_id,source_key"
    ).fetchall()
    by_external_fsi, by_external_fide = collections.defaultdict(set), collections.defaultdict(set)
    by_external_pair = collections.defaultdict(list)
    for row in external:
        source_hash(sources, row["source_key"])
        if row["source_sha256"] != source_hash(sources, row["source_key"]):
            raise ValueError("External ID-pair source fingerprint mismatch")
        pair = (row["fsi_id"], row["fide_id"])
        by_external_pair[pair].append(row["source_key"])
        by_external_fsi[row["fsi_id"]].add(row["fide_id"])
        by_external_fide[row["fide_id"]].add(row["fsi_id"])

    links = {(r["fsi_id"], r["fide_id"]): r for r in db.execute("SELECT * FROM identity_link")}
    certificates = {}
    for cert in db.execute("SELECT * FROM externally_certified_crosswalk"):
        pair = (cert["fsi_id"], cert["fide_id"])
        if pair in certificates:
            raise ValueError("Duplicate certificate")
        certificates[pair] = cert

    summary_states = collections.Counter()
    summary_reasons = collections.Counter()
    details = []
    observed_ids = set(official)
    official_corroborated = 0
    for r in assertions:
        sid, fid = r["fsi_id"], r["fide_id"]
        pair = (sid, fid)
        link = {"status": r["link_status"], "reason": r["link_reason"]}
        reasons = set()
        if len(by_fsi[sid]) > 1 or len(by_fide[fid]) > 1:
            reasons.add("ALLIN_NON_BIJECTIVE_ID")
        if any(f != fid for f in by_external_fsi.get(sid, ())):
            reasons.add("INDEPENDENT_ID_PAIR_FSI_CONFLICT")
        if any(s != sid for s in by_external_fide.get(fid, ())):
            reasons.add("INDEPENDENT_ID_PAIR_FIDE_CONFLICT")
        if r["link_status"] is None:
            reasons.add("LINK_RECORD_MISSING")
        elif r["link_status"] == "CONFLICT":
            reasons.add("SOURCE_LINK_CONFLICT")
        elif r["link_status"] not in ("VERIFIED", "REVIEW", "SOURCE_ASSERTED"):
            reasons.add("LINK_STATUS_UNRECOGNIZED")
        if r["fide_name"] is None:
            reasons.add("FIDE_ID_NOT_IN_CHECKED_SNAPSHOT")
        if r["fsi_name"] is None:
            reasons.add("FSI_ID_NOT_IN_LOCAL_INDEX")
        if r["fsi_name"] is not None and r["fide_name"] is not None:
            if (name_signature(r["fsi_name"]) != name_signature(r["fide_name"])):
                reasons.add("FSI_FIDE_NAME_DISAGREEMENT")
            if (r["fsi_birth_year"] is not None and r["fide_birth_year"] is not None
                    and r["fsi_birth_year"] != r["fide_birth_year"]):
                reasons.add("FSI_FIDE_BIRTH_YEAR_DISAGREEMENT")
        official_rows = official.get(sid, ())
        if official_rows:
            official_pairs = set()
            for ob in official_rows:
                source_hash(sources, ob["source_key"])
                if source_hash(sources, ob["source_key"]) != ob["source_sha256"]:
                    raise ValueError("Official FSI roster source fingerprint mismatch")
                official_pairs.add((name_signature(ob["full_name"]), ob["birth_year"]))
            if r["fsi_name"] is not None:
                sig = name_signature(r["fsi_name"])
                if not any(x[0] == sig for x in official_pairs):
                    reasons.add("FSI_OFFICIAL_NAME_DISAGREEMENT")
                elif r["fsi_birth_year"] is not None and not any(
                    x[0] == sig and x[1] == r["fsi_birth_year"] for x in official_pairs
                ):
                    reasons.add("FSI_OFFICIAL_BIRTH_YEAR_DISAGREEMENT")
                else:
                    official_corroborated += 1
        if r["link_status"] in ("REVIEW", "SOURCE_ASSERTED"):
            reasons.add("SOURCE_LINK_NEEDS_REVIEW")
        cert = certificates.get(pair)
        cert_problems = certificate_errors(cert, sources, link) if cert is not None else []
        if cert is not None and cert["tournament_source_key"] not in by_external_pair.get(pair, ()):
            cert_problems.append("CERTIFICATE_TOURNAMENT_PAIR_MISSING")
        reasons.update(cert_problems)
        if cert_problems:
            state = "CERTIFICATE_INVALID_REVIEW"
        elif reasons & {
            "ALLIN_NON_BIJECTIVE_ID", "INDEPENDENT_ID_PAIR_FSI_CONFLICT",
            "INDEPENDENT_ID_PAIR_FIDE_CONFLICT", "SOURCE_LINK_CONFLICT",
        }:
            state = "CONFLICT"
        elif reasons:
            state = "REVIEW_REQUIRED"
        elif cert is not None:
            state = "INDEPENDENTLY_CERTIFIED"
        else:
            state = "COMPATIBLE_BUT_UNCERTIFIED"
        summary_states[state] += 1
        for reason in sorted(reasons):
            summary_reasons[reason] += 1
        if state in ("REVIEW_REQUIRED", "CONFLICT", "CERTIFICATE_INVALID_REVIEW"):
            keys = {r["source_key"]}
            if r["fsi_source"] is not None:
                keys.add(r["fsi_source"])
            if r["fide_source"] is not None:
                keys.add(r["fide_source"])
            keys.update(by_external_pair.get(pair, ()))
            keys.update(row["source_key"] for row in official_rows)
            if cert is not None:
                keys.update((cert["official_fsi_source_key"],
                             cert["official_fide_source_key"],
                             cert["tournament_source_key"]))
            details.append({
                "fsi_id": sid,
                "fide_id": fid,
                "state": state,
                "reason_codes": sorted(reasons),
                "raw_link_status": r["link_status"],
                "raw_link_reason": r["link_reason"],
                "source_evidence": [
                    {"source_key": k, "sha256": source_hash(sources, k)}
                    for k in sorted(keys)
                ],
                "explicit_external_pair_observations": len(by_external_pair.get(pair, [])),
                "official_fsi_roster_observations": len(official_rows),
            })
    # Independent pairs absent from the older Allin snapshot are valid evidence
    # of a source gap; however, their certificate must still pass every check.
    for (sid, fid), cert in sorted(certificates.items()):
        if fid not in by_fsi.get(sid, set()):
            problems = certificate_errors(cert, sources, links.get((sid, fid)))
            has_tournament_evidence = cert["tournament_source_key"] in by_external_pair.get((sid, fid), ())
            if not has_tournament_evidence:
                problems.append("CERTIFICATE_TOURNAMENT_PAIR_MISSING")
            state = ("CERTIFICATE_INVALID_REVIEW" if problems
                     else "INDEPENDENTLY_CERTIFIED_OUTSIDE_ALLIN")
            reasons = sorted(set(problems + ["PAIR_ABSENT_FROM_ALLIN"]))
            link = links.get((sid, fid))
            details.append({
                "fsi_id": sid, "fide_id": fid,
                "state": state,
                "reason_codes": reasons,
                "raw_link_status": link["status"] if link is not None else None,
                "raw_link_reason": link["reason"] if link is not None else None,
                "source_evidence": [
                    {"source_key": k, "sha256": source_hash(sources, k)}
                    for k in sorted(set((cert["tournament_source_key"],
                                         cert["official_fsi_source_key"],
                                         cert["official_fide_source_key"])))
                ],
                "explicit_external_pair_observations": len(by_external_pair.get((sid, fid), [])),
                "official_fsi_roster_observations": len(official.get(sid, ())),
            })
            summary_states[state] += 1
            for reason in reasons:
                summary_reasons[reason] += 1
    candidate_status = dict(db.execute(
        "SELECT status,COUNT(*) FROM crosswalk_candidate GROUP BY status"
    ).fetchall())
    raw_link_status = dict(db.execute(
        "SELECT status,COUNT(*) FROM identity_link GROUP BY status"
    ).fetchall())
    report = {
        "profile": PROFILE,
        "scope": "local_pilot_source_assertions_not_italian_population",
        "policy": "three_distinct_independent_source_proofs_required; allin_cannot_certify",
        "rows_from_allin_crosswalk": len(assertions),
        "states": dict(sorted(summary_states.items())),
        "reason_counts": dict(sorted(summary_reasons.items())),
        "discrepancy_rows": len(details),
        "unlinked_candidates_by_status": dict(sorted(candidate_status.items())),
        "raw_identity_link_status": dict(sorted(raw_link_status.items())),
        "official_fsi_observation_rows": sum(map(len, official.values())),
        "official_fsi_ids_observed": len(observed_ids),
        "allin_pairs_with_official_fsi_name_year_agreement": official_corroborated,
        "explicit_external_id_pair_observations": len(external),
        "sources": {
            "allin": {
                "key": "allin-2026-04",
                "sha256": source_hash(sources, "allin-2026-04"),
                "currentness": "not_established",
            },
            "fide_checked_snapshot": {
                "key": "fide-current",
                "sha256": source_hash(sources, "fide-current"),
                "period_independently_verified": False,
            },
            "registered_source_count": len(sources),
        },
        "limitations": [
            "A matching FIDE record does not independently prove the FSI-FIDE ID pair.",
            "The official FSI roster observations are partial, not a complete national census.",
            "Missing FIDE IDs in a snapshot are review items, not proof of nonexistence.",
            "Legacy VERIFIED in identity_link is not equivalent to three-source certification.",
            "This report contains no person names, dates of birth, or full roster rows.",
        ],
    }
    details.sort(key=lambda r: (r["fsi_id"], r["fide_id"], r["state"]))
    return report, details


def ensure_private_output(path):
    repo_root = Path(__file__).resolve().parents[2]
    resolved = path.resolve()
    if resolved == repo_root or repo_root in resolved.parents:
        raise ValueError("Private audit output must be outside public repository")
    if path.is_symlink() or path.exists():
        raise ValueError("Audit output must be a NEW non-symlink path")
    if not path.parent.is_dir():
        raise ValueError("Audit output parent directory does not exist")


def write_outputs(summary_path, details_path, report, details):
    if summary_path.resolve() == details_path.resolve():
        raise ValueError("Summary and detail paths must differ")
    for p in (summary_path, details_path):
        ensure_private_output(p)
    # Both outputs are immutable/new: never silently overwrite a previous audit.
    with details_path.open("x", encoding="utf-8") as out:
        for row in details:
            out.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")
    with summary_path.open("x", encoding="utf-8") as out:
        out.write(json.dumps(report, ensure_ascii=False, sort_keys=True, indent=2) + "\n")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--db", type=Path, required=True)
    p.add_argument("--summary-json", type=Path, required=True)
    p.add_argument("--details-jsonl", type=Path, required=True)
    args = p.parse_args()
    with connect_readonly(args.db) as db:
        report, details = audit(db)
    write_outputs(args.summary_json, args.details_jsonl, report, details)
    print(json.dumps({
        "profile": PROFILE,
        "rows": report["rows_from_allin_crosswalk"],
        "states": report["states"],
        "discrepancy_rows": report["discrepancy_rows"],
        "summary": str(args.summary_json),
        "details": str(args.details_jsonl),
    }, ensure_ascii=False, sort_keys=True, indent=2))


if __name__ == "__main__":
    main()
