#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time

from collections import Counter
from datetime import date
from pathlib import Path
from typing import Any


TAG_RE = re.compile(r'^\[([A-Za-z0-9_]+)\s+"(.*)"\]\s*$')
DATE_RE = re.compile(r'^(\d{4}|\?{4})\.(\d{2}|\?{2})\.(\d{2}|\?{2})$')


RATING_BANDS = (
    ("under_1200", None, 1199),
    ("1200_1399", 1200, 1399),
    ("1400_1599", 1400, 1599),
    ("1600_1799", 1600, 1799),
    ("1800_1999", 1800, 1999),
    ("2000_2199", 2000, 2199),
    ("2200_2399", 2200, 2399),
    ("2400_plus", 2400, None),
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pgn", required=True, type=Path)
    parser.add_argument("--out-dir", required=True, type=Path)
    parser.add_argument("--progress-every", type=int, default=100_000)
    return parser.parse_args()


def parse_rating(raw: str | None) -> int | None:
    if raw is None:
        return None

    raw = raw.strip()

    if not raw.isdigit():
        return None

    rating = int(raw)

    if rating < 100 or rating > 4000:
        return None

    return rating


def rating_band(rating: int) -> str:
    for name, minimum, maximum in RATING_BANDS:
        if minimum is not None and rating < minimum:
            continue
        if maximum is not None and rating > maximum:
            continue
        return name

    return "invalid"


def parse_pgn_date(raw: str | None) -> tuple[str, date | None, int | None]:
    if raw is None or not raw.strip():
        return "missing", None, None

    raw = raw.strip()
    match = DATE_RE.fullmatch(raw)

    if match is None:
        return "invalid", None, None

    year_raw, month_raw, day_raw = match.groups()

    if year_raw == "????":
        return "unknown", None, None

    year = int(year_raw)

    if month_raw == "??" or day_raw == "??":
        return "partial", None, year

    try:
        parsed = date(year, int(month_raw), int(day_raw))
    except ValueError:
        return "invalid", None, year

    return "full", parsed, year


def percentage(part: int | float, whole: int) -> float:
    return 0.0 if whole == 0 else 100.0 * float(part) / whole


def main() -> int:
    args = parse_args()
    pgn = args.pgn.expanduser().resolve()
    out_dir = args.out_dir.expanduser().resolve()

    if not pgn.is_file():
        print(f"ERRORE: PGN non trovato: {pgn}", file=sys.stderr)
        return 1

    out_dir.mkdir(parents=True, exist_ok=True)

    started = time.monotonic()
    file_size = pgn.stat().st_size

    total_games = 0
    malformed_tag_lines = 0

    date_status = Counter()
    year_counts = Counter()
    result_counts = Counter()
    rating_counts = Counter()
    rating_band_counts = Counter()

    both_ratings = 0
    one_rating = 0
    no_ratings = 0

    nonstandard_start = 0
    missing_result = 0
    missing_names = 0

    chronological_comparisons = 0
    chronological_regressions = 0
    equal_consecutive_dates = 0
    maximum_backward_days = 0

    earliest_date: date | None = None
    latest_date: date | None = None
    previous_full_date: date | None = None

    headers: dict[str, str] = {}

    def process_game(game_headers: dict[str, str]) -> None:
        nonlocal total_games
        nonlocal both_ratings, one_rating, no_ratings
        nonlocal nonstandard_start, missing_result, missing_names
        nonlocal chronological_comparisons
        nonlocal chronological_regressions
        nonlocal equal_consecutive_dates
        nonlocal maximum_backward_days
        nonlocal earliest_date, latest_date, previous_full_date

        if not game_headers:
            return

        total_games += 1

        status, full_date, year = parse_pgn_date(game_headers.get("Date"))
        date_status[status] += 1

        if year is not None:
            year_counts[str(year)] += 1

        if full_date is not None:
            if earliest_date is None or full_date < earliest_date:
                earliest_date = full_date

            if latest_date is None or full_date > latest_date:
                latest_date = full_date

            if previous_full_date is not None:
                chronological_comparisons += 1

                if full_date < previous_full_date:
                    chronological_regressions += 1
                    backward = (previous_full_date - full_date).days
                    maximum_backward_days = max(maximum_backward_days, backward)
                elif full_date == previous_full_date:
                    equal_consecutive_dates += 1

            previous_full_date = full_date

        white_rating = parse_rating(game_headers.get("WhiteElo"))
        black_rating = parse_rating(game_headers.get("BlackElo"))

        valid_ratings = [rating for rating in (white_rating, black_rating) if rating]

        if len(valid_ratings) == 2:
            both_ratings += 1
        elif len(valid_ratings) == 1:
            one_rating += 1
        else:
            no_ratings += 1

        for colour, rating in (
            ("white", white_rating),
            ("black", black_rating),
        ):
            if rating is None:
                raw = game_headers.get(f"{colour.title()}Elo")

                if raw is None or not raw.strip():
                    rating_counts[f"{colour}_missing"] += 1
                else:
                    rating_counts[f"{colour}_invalid"] += 1
            else:
                rating_counts[f"{colour}_valid"] += 1
                rating_band_counts[rating_band(rating)] += 1

        result = game_headers.get("Result")

        if result is None:
            missing_result += 1
            result_counts["missing"] += 1
        else:
            result_counts[result] += 1

        if not game_headers.get("White") or not game_headers.get("Black"):
            missing_names += 1

        if game_headers.get("SetUp") == "1" or "FEN" in game_headers:
            nonstandard_start += 1

        if total_games % args.progress_every == 0:
            elapsed = max(time.monotonic() - started, 0.001)
            rate = total_games / elapsed
            print(
                f"[AUDIT] {total_games:,} partite | "
                f"{rate:,.0f} partite/s | "
                f"date complete={date_status['full']:,} | "
                f"due rating={both_ratings:,}",
                flush=True,
            )

    print(f"[AUDIT] PGN: {pgn}", flush=True)
    print(f"[AUDIT] Dimensione: {file_size / (1024 ** 3):.2f} GiB", flush=True)

    with pgn.open("r", encoding="utf-8", errors="replace", newline="") as handle:
        for raw_line in handle:
            line = raw_line.rstrip("\r\n")

            if line.startswith("[Event ") and headers:
                process_game(headers)
                headers = {}

            if line.startswith("["):
                match = TAG_RE.match(line)

                if match is None:
                    malformed_tag_lines += 1
                    continue

                key, value = match.groups()
                headers[key] = value

    process_game(headers)

    elapsed = time.monotonic() - started

    report: dict[str, Any] = {
        "pgn": str(pgn),
        "file_size_bytes": file_size,
        "elapsed_seconds": elapsed,
        "total_games": total_games,
        "malformed_tag_lines": malformed_tag_lines,
        "dates": {
            "status": dict(sorted(date_status.items())),
            "earliest_full": earliest_date.isoformat() if earliest_date else None,
            "latest_full": latest_date.isoformat() if latest_date else None,
            "years": dict(sorted(year_counts.items())),
            "chronological_comparisons": chronological_comparisons,
            "chronological_regressions": chronological_regressions,
            "chronological_regression_percent": percentage(
                chronological_regressions,
                chronological_comparisons,
            ),
            "equal_consecutive_dates": equal_consecutive_dates,
            "maximum_backward_jump_days": maximum_backward_days,
        },
        "ratings": {
            "games_with_both": both_ratings,
            "games_with_one": one_rating,
            "games_with_none": no_ratings,
            "games_with_both_percent": percentage(both_ratings, total_games),
            "field_quality": dict(sorted(rating_counts.items())),
            "individual_ratings_by_band": dict(sorted(rating_band_counts.items())),
        },
        "headers": {
            "nonstandard_start_games": nonstandard_start,
            "missing_result_games": missing_result,
            "missing_player_name_games": missing_names,
            "results": dict(sorted(result_counts.items())),
        },
    }

    json_path = out_dir / "audit.json"
    text_path = out_dir / "audit.txt"

    json_path.write_text(
        json.dumps(report, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )

    lines = [
        "ANTISKILL THEORY — AUDIT CORPUS",
        "===============================",
        f"PGN: {pgn}",
        f"Dimensione: {file_size / (1024 ** 3):.2f} GiB",
        f"Tempo scansione: {elapsed:.1f} secondi",
        f"Partite totali: {total_games:,}",
        "",
        "DATE",
        f"- complete: {date_status['full']:,} "
        f"({percentage(date_status['full'], total_games):.2f}%)",
        f"- parziali: {date_status['partial']:,}",
        f"- sconosciute: {date_status['unknown']:,}",
        f"- mancanti: {date_status['missing']:,}",
        f"- invalide: {date_status['invalid']:,}",
        f"- prima data completa: {earliest_date}",
        f"- ultima data completa: {latest_date}",
        f"- confronti cronologici: {chronological_comparisons:,}",
        f"- regressioni cronologiche: {chronological_regressions:,} "
        f"({percentage(chronological_regressions, chronological_comparisons):.3f}%)",
        f"- massimo salto all'indietro: {maximum_backward_days:,} giorni",
        "",
        "RATING",
        f"- partite con entrambi i rating: {both_ratings:,} "
        f"({percentage(both_ratings, total_games):.2f}%)",
        f"- partite con un solo rating: {one_rating:,}",
        f"- partite senza rating validi: {no_ratings:,}",
        "- rating individuali per fascia:",
    ]

    for band, count in sorted(rating_band_counts.items()):
        lines.append(f"  {band}: {count:,}")

    lines.extend(
        [
            "",
            "ALTRE INTESTAZIONI",
            f"- partite da posizione non standard: {nonstandard_start:,}",
            f"- risultato mancante: {missing_result:,}",
            f"- nome giocatore mancante: {missing_names:,}",
            f"- righe tag malformate: {malformed_tag_lines:,}",
            "",
            "DISTRIBUZIONE PER ANNO",
        ]
    )

    for year, count in sorted(year_counts.items()):
        lines.append(f"- {year}: {count:,}")

    lines.extend(
        [
            "",
            f"JSON completo: {json_path}",
            f"Report: {text_path}",
        ]
    )

    text_path.write_text("\n".join(lines) + "\n", encoding="utf-8")

    print()
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
