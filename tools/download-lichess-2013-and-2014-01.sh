#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$HOME/progetti/antiskill"
DEST="$ROOT/data/lichess/standard"
BASE="https://database.lichess.org/standard"

cd "$DEST"

command -v curl >/dev/null || {
    echo "ERRORE: curl non installato."
    exit 1
}

echo "===== MANIFEST SHA-256 ====="

curl \
    --fail \
    --location \
    --retry 20 \
    --retry-delay 10 \
    --connect-timeout 30 \
    --output sha256sums.txt \
    "$BASE/sha256sums.txt"

download_one() {
    local ym="$1"
    local file="lichess_db_standard_rated_${ym}.pgn.zst"
    local part="${file}.part"

    echo
    echo "===== $file ====="

    if [[ -f "$file" ]] &&
       grep -E "  ${file}$" sha256sums.txt |
       sha256sum -c - >/dev/null 2>&1
    then
        echo "Già presente e verificato."
        return
    fi

    if [[ -f "$file" && ! -f "$part" ]]; then
        mv "$file" "$part"
    fi

    curl \
        --fail \
        --location \
        --retry 20 \
        --retry-delay 10 \
        --connect-timeout 30 \
        --continue-at - \
        --output "$part" \
        "$BASE/$file"

    mv "$part" "$file"

    grep -E "  ${file}$" sha256sums.txt |
        sha256sum -c -
}

for month in {01..12}; do
    download_one "2013-$month"
done

download_one "2014-01"

grep -E \
    '  lichess_db_standard_rated_(2013-(0[1-9]|1[0-2])|2014-01)\.pgn\.zst$' \
    sha256sums.txt > wanted.sha256

COUNT="$(wc -l < wanted.sha256)"

if [[ "$COUNT" -ne 13 ]]; then
    echo "ERRORE: trovati $COUNT checksum invece di 13."
    exit 1
fi

echo
echo "===== VERIFICA FINALE COMPLETA ====="
sha256sum -c wanted.sha256 | tee verification.txt

{
    echo "Completato: $(date --iso-8601=seconds)"
    echo "File verificati: 13"
    du -ch ./*.pgn.zst | tail -n1
} | tee DOWNLOAD-COMPLETE.txt

echo
echo "===== DOWNLOAD COMPLETATO ====="
cat DOWNLOAD-COMPLETE.txt
