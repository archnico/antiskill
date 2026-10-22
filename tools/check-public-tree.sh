#!/usr/bin/env bash
set -euo pipefail

# Generic public-tree guard. It intentionally does not embed private hostnames,
# usernames or local paths in the repository it is meant to protect.

python3 - <<'PY'
from pathlib import Path
import re
import subprocess
import sys

root = Path.cwd().resolve()
failed = False

# Public licensing contract: keep the project on the deliberately chosen
# EUPL-1.2 + DCO baseline. Data rights remain separate from software licensing.
cargo_toml = Path("Cargo.toml").read_text(encoding="utf-8")
if not re.search(r'^license\s*=\s*"EUPL-1\.2"\s*$', cargo_toml, flags=re.M):
    print("Public-tree safety: Cargo.toml must declare license = \"EUPL-1.2\"", file=sys.stderr)
    failed = True
if not Path("LICENSE").is_file():
    print("Public-tree safety: LICENSE is missing", file=sys.stderr)
    failed = True
if not Path("DCO").is_file():
    print("Public-tree safety: DCO is missing", file=sys.stderr)
    failed = True
if not Path("SECURITY.md").is_file():
    print("Public-tree safety: SECURITY.md is missing", file=sys.stderr)
    failed = True

# Third-party GitHub Actions must use immutable 40-hex commit SHAs. Mutable
# tags such as @v4 or branches such as @main make public CI supply-chain state
# change without a repository commit.
action_use = re.compile(r"(?m)^\s*-?\s*uses:\s*([^\s#]+)@([^\s#]+)")
sha40 = re.compile(r"^[0-9a-fA-F]{40}$")
for workflow in sorted(Path(".github/workflows").glob("*.y*ml")):
    text = workflow.read_text(encoding="utf-8")
    for action, ref in action_use.findall(text):
        if action.startswith("./") or action.startswith("docker://"):
            continue
        if not sha40.fullmatch(ref):
            print(
                f"Public-tree safety: mutable GitHub Action ref in {workflow}: {action}@{ref}",
                file=sys.stderr,
            )
            failed = True

tracked_raw = subprocess.check_output(["git", "ls-files", "-z"]).split(b"\0")
tracked = [Path(p.decode("utf-8")) for p in tracked_raw if p]

# Evidence known to belong outside the public current tree. These filenames are
# technical history identifiers and contain no private person names themselves.
forbidden_paths = {
    "docs/issue-87-metadata-representative-acceptance-20260913.md",
    "docs/issue-88-player-identity-acceptance-20260913.md",
    "docs/issue-91-player-identity-federation-v2-acceptance-20260913.md",
    "docs/issue-93-tpol-official-context-v1-acceptance-20260913.md",
    "docs/issue-95-context-registry-v1-acceptance-20260913.md",
}
for path in tracked:
    if path.as_posix() in forbidden_paths:
        print(f"Public-tree safety: private evidence path is tracked: {path}", file=sys.stderr)
        failed = True

# Tracked symlinks must stay inside the repository and resolve to a real target.
index_records = subprocess.check_output(["git", "ls-files", "-s", "-z"]).split(b"\0")
for record in index_records:
    if not record:
        continue
    header, raw_path = record.split(b"\t", 1)
    mode = header.split(b" ", 1)[0]
    if mode != b"120000":
        continue
    path = Path(raw_path.decode("utf-8"))
    target = subprocess.check_output(["git", "show", f":{path.as_posix()}"]).decode("utf-8").strip()
    resolved = (root / path.parent / target).resolve(strict=False)
    try:
        resolved.relative_to(root)
    except ValueError:
        print(f"Public-tree safety: symlink escapes repository: {path}", file=sys.stderr)
        failed = True
        continue
    if not resolved.exists():
        print(f"Public-tree safety: dangling tracked symlink: {path}", file=sys.stderr)
        failed = True

# Catch accidental corpora or binary dumps early. Large public artifacts need an
# explicit packaging decision instead of silently entering Git history.
MAX_TRACKED_BYTES = 5 * 1024 * 1024
for path in tracked:
    try:
        size = path.stat().st_size
    except OSError:
        continue
    if size > MAX_TRACKED_BYTES:
        print(f"Public-tree safety: tracked file exceeds 5 MiB: {path} ({size} bytes)", file=sys.stderr)
        failed = True
    if path.suffix.lower() == ".pgn" and not path.as_posix().startswith("tests/fixtures/"):
        print(f"Public-tree safety: PGN corpus outside tests/fixtures: {path}", file=sys.stderr)
        failed = True

secret_patterns = {
    "private key": re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
    "GitHub token": re.compile(r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{30,}\b|\bgithub_pat_[A-Za-z0-9_]{40,}\b"),
    "OpenAI key": re.compile(r"\bsk-[A-Za-z0-9_-]{20,}\b"),
    "Google API key": re.compile(r"\bAIza[0-9A-Za-z_-]{30,}\b"),
    "AWS access key": re.compile(r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b"),
    "Slack token": re.compile(r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b"),
    "literal secret assignment": re.compile(
        r"(?i)\b(?:password|passwd|api[_-]?key|secret[_-]?key|access[_-]?token)\b\s*[:=]\s*[\"'][^\"'\n]{8,}[\"']"
    ),
}

posix_home = re.compile(r"/(?:home|Users)/[^/\s]+/")
windows_home = re.compile(r"[A-Za-z]:\\Users\\[^\\\s]+\\")
email = re.compile(r"(?<![\w.+-])([A-Z0-9._%+-]+)@([A-Z0-9.-]+\.[A-Z]{2,})(?![\w.-])", re.I)
allowed_email_domains = {"example.com", "users.noreply.github.com"}

for path in tracked:
    if path.as_posix() == "tools/check-public-tree.sh":
        continue
    try:
        data = path.read_bytes()
    except OSError:
        continue
    if b"\0" in data:
        continue
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        continue

    if posix_home.search(text) or windows_home.search(text):
        print(f"Public-tree safety: absolute user-home path in {path}", file=sys.stderr)
        failed = True

    for match in email.finditer(text):
        domain = match.group(2).lower()
        if domain not in allowed_email_domains:
            print(f"Public-tree safety: non-example email address in {path}", file=sys.stderr)
            failed = True
            break

    for label, pattern in secret_patterns.items():
        if pattern.search(text):
            print(f"Public-tree safety: possible {label} in {path}", file=sys.stderr)
            failed = True

if failed:
    raise SystemExit(1)
PY

echo 'PUBLIC_TREE_SAFETY=PASS'
