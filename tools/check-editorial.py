#!/usr/bin/env python3
"""Small guard against public-doc drift and accidental bloat."""

from pathlib import Path
import re
import sys

pairs = [
    (Path("README.md"), Path("LEGGIMI.md"), 170),
    (Path("CONTRIBUTING.md"), Path("CONTRIBUIRE.md"), 140),
]
areas = [
    "Nucleo",
    "Ricerca",
    "Rapporti",
    "Analisi",
    "Fonti",
    "Interoperabilità",
    "Conoscenza",
    "Piattaforma",
    "Laboratorio",
    "Qualità",
]
failed = False

for left, right, max_lines in pairs:
    left_text = left.read_text(encoding="utf-8")
    right_text = right.read_text(encoding="utf-8")
    left_lines = left_text.count("\n") + 1
    right_lines = right_text.count("\n") + 1

    if left_lines > max_lines or right_lines > max_lines:
        print(
            f"Editorial: public entry point exceeds {max_lines} lines: "
            f"{left}={left_lines}, {right}={right_lines}",
            file=sys.stderr,
        )
        failed = True

    left_h2 = re.findall(r"^## .+$", left_text, flags=re.M)
    right_h2 = re.findall(r"^## .+$", right_text, flags=re.M)
    if len(left_h2) != len(right_h2):
        print(
            f"Editorial: bilingual section-count drift: {left}={len(left_h2)}, "
            f"{right}={len(right_h2)}",
            file=sys.stderr,
        )
        failed = True

    for area in areas:
        if area not in left_text or area not in right_text:
            print(f"Editorial: area '{area}' missing from {left} or {right}", file=sys.stderr)
            failed = True

entry_files = [
    Path("README.md"),
    Path("LEGGIMI.md"),
    Path("CONTRIBUTING.md"),
    Path("CONTRIBUIRE.md"),
    Path("docs/MAPPA.md"),
    Path("docs/ARCHITETTURA.md"),
    Path("docs/CONTRIBUTORI.md"),
    Path("docs/ROADMAP.md"),
    Path("docs/PUBBLICAZIONE.md"),
    Path("docs/PRESENTAZIONE.md"),
    Path("docs/POSSIBILITA.md"),
    Path("docs/PRIVACY.md"),
    Path("docs/LICENZE.md"),
    Path("docs/RAPPORTI.md"),
    Path("docs/INDICE.md"),
    Path("docs/SEPARAZIONE.md"),
]

for path in entry_files:
    text = path.read_text(encoding="utf-8")
    legacy_label = "area:" + "archivio"
    if legacy_label in text:
        print(f"Editorial: legacy area label in {path}", file=sys.stderr)
        failed = True
    if re.search(r"\bArchivio\b", text):
        print(f"Editorial: legacy area name in {path}", file=sys.stderr)
        failed = True

if failed:
    raise SystemExit(1)

print("EDITORIAL=PASS")
