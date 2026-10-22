#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import re
import time

from collections import Counter
from pathlib import Path
from typing import Any


TAG_RE = re.compile(
    r'^\[([A-Za-z0-9_]+)\s+"((?:\\.|[^"\\])*)"\]\s*$'
)

MAX_SAMPLES = 40


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pgn", required=True, type=Path)
    parser.add_argument("--out-dir", required=True, type=Path)
    parser.add_argument("--progress-every", type=int, default=1_000_000)
    return parser.parse_args()


def valid_rating(raw: str | None) -> bool:
    if raw is None:
        return False

    raw = raw.strip()

    if not raw.isdigit():
        return False

    value = int(raw)
    return 100 <= value <= 4000


def classify_malformed(line: str) -> str:
    stripped = line.strip()

    if not stripped.endswith("]"):
        return "missing_closing_bracket"

    if '"' not in stripped:
        return "no_quotes"

    if stripped.count('"') % 2 != 0:
        return "odd_quote_count"

    token = stripped[1:].split(maxsplit=1)[0] if len(stripped) > 1 else ""

    if token:
        return f"token:{token[:40]}"

    return "other"


def main() -> int:
    args = parse_args()

    pgn = args.pgn.expanduser().resolve()
    out_dir = args.out_dir.expanduser().resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    started = time.monotonic()

    total_games = 0
    games_2024 = 0

    headers: dict[str, str] = {}
    in_header_section = False

    malformed_total = 0
    malformed_header = 0
    malformed_movetext = 0

    malformed_classes: Counter[str] = Counter()
    malformed_tokens: Counter[str] = Counter()

    malformed_samples: list[dict[str, Any]] = []
    samples_2024: list[dict[str, str]] = []

    fields_2024: Counter[str] = Counter()
    time_controls_2024: Counter[str] = Counter()
    events_2024: Counter[str] = Counter()
    sites_2024: Counter[str] = Counter()
    results_2024: Counter[str] = Counter()

    both_ratings_2024 = 0
    one_rating_2024 = 0
    no_ratings_2024 = 0
    setup_games_2024 = 0
    complete_dates_2024 = 0
    partial_dates_2024 = 0

    def process_game(game_headers: dict[str, str]) -> None:
        nonlocal total_games
        nonlocal games_2024
        nonlocal both_ratings_2024
        nonlocal one_rating_2024
        nonlocal no_ratings_2024
        nonlocal setup_games_2024
        nonlocal complete_dates_2024
        nonlocal partial_dates_2024

        if not game_headers:
            return

        total_games += 1

        raw_date = game_headers.get("Date", "")

        if not raw_date.startswith("2024."):
            return

        games_2024 += 1

        for field in (
            "Event",
            "Site",
            "Date",
            "Round",
            "White",
            "Black",
            "Result",
            "WhiteElo",
            "BlackElo",
            "ECO",
            "Opening",
            "Variation",
            "TimeControl",
            "SetUp",
            "FEN",
        ):
            if game_headers.get(field):
                fields_2024[field] += 1

        if re.fullmatch(r"2024\.\d{2}\.\d{2}", raw_date):
            complete_dates_2024 += 1
        else:
            partial_dates_2024 += 1

        white_ok = valid_rating(game_headers.get("WhiteElo"))
        black_ok = valid_rating(game_headers.get("BlackElo"))

        if white_ok and black_ok:
            both_ratings_2024 += 1
        elif white_ok or black_ok:
            one_rating_2024 += 1
        else:
            no_ratings_2024 += 1

        if game_headers.get("SetUp") == "1" or "FEN" in game_headers:
            setup_games_2024 += 1

        time_controls_2024[game_headers.get("TimeControl", "<missing>")] += 1
        events_2024[game_headers.get("Event", "<missing>")] += 1
        sites_2024[game_headers.get("Site", "<missing>")] += 1
        results_2024[game_headers.get("Result", "<missing>")] += 1

        if len(samples_2024) < 30:
            samples_2024.append(
                {
                    key: game_headers.get(key, "")
                    for key in (
                        "Event",
                        "Site",
                        "Date",
                        "Round",
                        "White",
                        "Black",
                        "Result",
                        "WhiteElo",
                        "BlackElo",
                        "ECO",
                        "Opening",
                        "TimeControl",
                        "SetUp",
                        "FEN",
                    )
                }
            )

    print(f"[INSPECT] PGN: {pgn}", flush=True)
    print(
        f"[INSPECT] Dimensione: {pgn.stat().st_size / (1024 ** 3):.2f} GiB",
        flush=True,
    )

    with pgn.open(
        "r",
        encoding="utf-8",
        errors="replace",
        newline="",
    ) as handle:
        for line_number, raw_line in enumerate(handle, start=1):
            line = raw_line.rstrip("\r\n")

            if line.startswith("["):
                match = TAG_RE.fullmatch(line)

                if match is not None:
                    key, value = match.groups()

                    if key == "Event" and headers:
                        process_game(headers)
                        headers = {}

                    headers[key] = value
                    in_header_section = True
                    continue

                malformed_total += 1

                location = "header" if in_header_section else "movetext"

                if in_header_section:
                    malformed_header += 1
                else:
                    malformed_movetext += 1

                category = classify_malformed(line)
                malformed_classes[category] += 1

                token = (
                    line[1:].split(maxsplit=1)[0][:60]
                    if len(line) > 1
                    else "<empty>"
                )
                malformed_tokens[token] += 1

                if len(malformed_samples) < MAX_SAMPLES:
                    malformed_samples.append(
                        {
                            "line_number": line_number,
                            "location": location,
                            "category": category,
                            "text": line[:500],
                        }
                    )

                continue

            if not line.strip() and headers:
                in_header_section = False

            if (
                total_games > 0
                and total_games % args.progress_every == 0
                and line.startswith("1.")
            ):
                elapsed = max(time.monotonic() - started, 0.001)
                print(
                    f"[INSPECT] circa {total_games:,} partite | "
                    f"2024={games_2024:,} | "
                    f"anomalie={malformed_total:,} | "
                    f"{total_games / elapsed:,.0f} partite/s",
                    flush=True,
                )

    process_game(headers)

    elapsed = time.monotonic() - started

    report = {
        "pgn": str(pgn),
        "elapsed_seconds": elapsed,
        "total_games": total_games,
        "games_2024": games_2024,
        "malformed": {
            "total": malformed_total,
            "header": malformed_header,
            "movetext": malformed_movetext,
            "classes": malformed_classes.most_common(),
            "tokens": malformed_tokens.most_common(100),
            "samples": malformed_samples,
        },
        "year_2024": {
            "complete_dates": complete_dates_2024,
            "partial_dates": partial_dates_2024,
            "both_ratings": both_ratings_2024,
            "one_rating": one_rating_2024,
            "no_ratings": no_ratings_2024,
            "nonstandard_start": setup_games_2024,
            "field_presence": dict(fields_2024),
            "time_controls": time_controls_2024.most_common(30),
            "events": events_2024.most_common(30),
            "sites": sites_2024.most_common(30),
            "results": results_2024.most_common(),
            "header_samples": samples_2024,
        },
    }

    json_path = out_dir / "inspection.json"
    text_path = out_dir / "inspection.txt"
    malformed_path = out_dir / "malformed-samples.tsv"
    headers_path = out_dir / "headers-2024-samples.json"

    json_path.write_text(
        json.dumps(report, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )

    with malformed_path.open("w", encoding="utf-8") as output:
        output.write("line_number\tlocation\tcategory\ttext\n")

        for sample in malformed_samples:
            text = (
                sample["text"]
                .replace("\\", "\\\\")
                .replace("\t", "\\t")
                .replace("\n", "\\n")
            )

            output.write(
                f"{sample['line_number']}\t"
                f"{sample['location']}\t"
                f"{sample['category']}\t"
                f"{text}\n"
            )

    headers_path.write_text(
        json.dumps(samples_2024, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )

    lines = [
        "ANTISKILL THEORY — ISPEZIONE CORPUS",
        "====================================",
        f"PGN: {pgn}",
        f"Tempo: {elapsed:.1f} secondi",
        f"Partite riconosciute: {total_games:,}",
        f"Partite 2024: {games_2024:,}",
        "",
        "RIGHE CON '[' NON RICONOSCIUTE COME TAG",
        f"- totale: {malformed_total:,}",
        f"- durante le intestazioni: {malformed_header:,}",
        f"- nel movetext/commenti: {malformed_movetext:,}",
        "",
        "Categorie principali:",
    ]

    for category, count in malformed_classes.most_common(20):
        lines.append(f"- {category}: {count:,}")

    lines.extend(
        [
            "",
            "QUALITÀ PARTITE 2024",
            f"- date complete: {complete_dates_2024:,}",
            f"- date parziali: {partial_dates_2024:,}",
            f"- entrambi i rating validi: {both_ratings_2024:,}",
            f"- un solo rating valido: {one_rating_2024:,}",
            f"- nessun rating valido: {no_ratings_2024:,}",
            f"- posizione iniziale non standard: {setup_games_2024:,}",
            "",
            "Presenza campi nel 2024:",
        ]
    )

    for field, count in sorted(fields_2024.items()):
        percentage = 100 * count / games_2024 if games_2024 else 0
        lines.append(f"- {field}: {count:,} ({percentage:.2f}%)")

    lines.extend(
        [
            "",
            "TimeControl più frequenti:",
        ]
    )

    for value, count in time_controls_2024.most_common(15):
        lines.append(f"- {value}: {count:,}")

    lines.extend(
        [
            "",
            f"JSON completo: {json_path}",
            f"Esempi anomalie: {malformed_path}",
            f"Esempi intestazioni 2024: {headers_path}",
        ]
    )

    text_path.write_text("\n".join(lines) + "\n", encoding="utf-8")

    print()
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
