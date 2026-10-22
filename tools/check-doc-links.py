#!/usr/bin/env python3
"""Fail when a tracked Markdown file contains a broken local link."""

from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import unquote

root = Path.cwd().resolve()
markdown = [
    Path(p.decode("utf-8"))
    for p in subprocess.check_output(["git", "ls-files", "-z", "*.md"]).split(b"\0")
    if p
]

link_re = re.compile(r"(?<!!)\[[^\]]*\]\(([^)]+)\)")
failed = False

for source in markdown:
    try:
        text = source.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        continue

    for raw in link_re.findall(text):
        target = raw.strip()
        if target.startswith("<") and ">" in target:
            target = target[1 : target.index(">")]
        elif " " in target:
            target = target.split(" ", 1)[0]

        if not target or target.startswith(("#", "http://", "https://", "mailto:")):
            continue

        target = unquote(target.split("#", 1)[0])
        if not target:
            continue

        resolved = (source.parent / target).resolve(strict=False)
        try:
            resolved.relative_to(root)
        except ValueError:
            print(f"Doc links: link escapes repository: {source} -> {target}", file=sys.stderr)
            failed = True
            continue

        if not resolved.exists():
            print(f"Doc links: broken local link: {source} -> {target}", file=sys.stderr)
            failed = True

if failed:
    raise SystemExit(1)

print(f"DOC_LINKS=PASS ({len(markdown)} markdown files)")
