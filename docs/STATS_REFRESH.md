# Antiskill Stats · Aggiornamento e provenienza

## Architettura

Coordinatore **n2**. `antiskill stats` resta una CLI read-only con filtri strutturati; lo scheduler usa `scripts/stats_maintenance.py`.
La sincronizzazione è quotidiana con timer systemd utente alle 06:20 (+ jitter fino a 3 minuti). Non pubblica né redistribuisce dati personali e non imposta mai `certified=true`.

1. Esporta soltanto il roster FSI per regione TOS e il circolo 17029 (anno corrente), tramite il connettore FSI autorizzato.
2. Conserva oggetti BLAKE3, manifest e snapshot precedenti. Se lo scaricamento fallisce, il precedente resta disponibile, ma lo stato espone l'errore; il report corrente non viene sostituito.
3. Esporta dal registro SQLite attuale soltanto i collegamenti FSI–FIDE VERIFIED, in un nuovo CSV privato con provenienza SHA256, senza sovrascrivere versioni precedenti. La build fallisce chiusa se il registro non è coerente.
4. Esamina l'inbox privata di ZIP FIDE già ottenuti lecitamente. Il download indiscriminato non viene eseguito.
5. Per il mese corrente con lista FIDE combinata e liste FIDE del mese precedente, calcola tre report di consultazione: Top3 Toscana Standard, Firenze Standard e Firenze Rapid U18. Genera inoltre una vista integrale dei ranghi regionali in `support/top3-tos-standard-full.json`, per confronti futuri senza attribuire ingressi da elenchi Top 3 troncati. La lista precedente può essere combined o standalone per Standard/Rapid; la variazione Elo è la differenza delle liste FIDE ufficiali, mai Allin.
6. Ogni report deve superare controllo schema, mese, provenienza FSI/FIDE e mancata certificazione. Il set completo viene promosso tramite symlink atomico `reports/rankings-YYYY-MM-managed`. Versioni precedenti immutabili in `reports/versions/YYYY-MM/`.
7. Gli aggiornamenti FSI non cambiano i report mensili se il contenuto BLAKE3 non cambia. La CLI `members` usa il roster FSI più recente. Report diversi o incompleti non vengono selezionati silenziosamente.

## Comandi

```bash
antiskill stats freshness
antiskill stats top3 --region TOS
antiskill stats club --club 17029 --rating standard
antiskill stats members --region TOS --year 2026 --by club,type
systemctl --user status antiskill-stats-refresh.timer
systemctl --user start antiskill-stats-refresh.service
journalctl --user -u antiskill-stats-refresh.service -n 100
python3 scripts/stats_maintenance.py status
python3 scripts/stats_maintenance.py build --month 2026-11
```

Per installare lo stesso servizio sul coordinatore n2 (percorso codice e archivio devono esistere):

```bash
install -Dm644 systemd/antiskill-stats-refresh.service ~/.config/systemd/user/antiskill-stats-refresh.service
install -Dm644 systemd/antiskill-stats-refresh.timer ~/.config/systemd/user/antiskill-stats-refresh.timer
systemctl --user daemon-reload
systemctl --user enable --now antiskill-stats-refresh.timer
```

## Import locale FIDE per i prossimi mesi

Creare la directory privata `~/.local/state/antiskill-stats/fide-inbox`, poi collocare il file ZIP e un file JSON separato, per esempio `2026-12-combined.json`:

```json
{
  "month": "2026-12",
  "kind": "combined",
  "file": "players_list_xml_legacy_dec2026.zip",
  "url": "https://ratings.fide.com/download/FILE_UFFICIALE.zip",
  "sha256": "IMPRONTA_SHA256_REALE_DEL_FILE",
  "approved_for_internal_use": true
}
```

Il valore `url` deve essere esattamente il link FIDE reale del file, non quello fittizio dell'esempio. Il campo `approved_for_internal_use` richiede una valutazione esplicita dei diritti d'uso. Il timer importa esclusivamente ZIP con SHA256 coincidente; se per lo stesso mese esiste già una versione dal contenuto diverso, si arresta per la verifica manuale. La provenienza dichiarata nel manifest non equivale a certificazione dell'autenticità FIDE.

## Copertura e limitazioni

- Dopo l'aggiornamento FIDE, i rating mensili e delta Standard/Rapid hanno origine nelle liste ufficiali archiviate; differenze di posizione e nuovi ingressi nel Top3 richiedono roster storici completi. Nessuna posizione precedente inventata.
- Il conteggio FSI è per anno di tessera e per regione di affiliazione del circolo, non per residenza; `--by residence-province` permette la vista separata.
- Le classifiche vengono archiviate solo quando tutte le tre viste superano i controlli. Non si attribuisce il valore "aggiornato" a un mese non disponibile.
- `antiskill stats freshness` mostra l'ultimo refresh e mese disponibile; è l'indicatore operativo, non un'approvazione editoriale.
- L'inbox non è uno scraper. I file FIDE vanno forniti con provenienza e diritti adeguati. Il timer non scarica automaticamente da FIDE.

## Confronto delle posizioni

Il report regionale integrale di novembre 2026 è conservato separatamente in `reports/full-snapshots/2026-11/top3-tos-standard-full.json`. I prossimi report gestiti includono il loro snapshot integrale dentro `support/`. Quando il file integrale precedente esiste, la pipeline prova a calcolare **tutte le 12 viste di movimento** (U08–U18, assoluta e femminile) in `support/movement-uNN-absolute.json` e `support/movement-uNN-female.json`. Gli errori restano nel campo `movement.failures`, senza inventare movimenti o bloccare i report Elo corretti; se manca la fonte precedente resta `pending`. Solo quando esiste uno snapshot equivalente del mese precedente (roster FSI rilevato nello stesso mese e report FIDE ufficiale completo) si può eseguire:

```bash
antiskill stats movement --current /percorso/full-2026-12.json --previous /percorso/full-2026-11.json --cohort U16 --sex female --format json
```

Un Top 3 con tre giocatori per fascia **non** basta: `movement` lo rifiuta. Assenze dalle liste complete vengono indicate come **unknown**, non come ingresso/uscita certa. Si confrontano solo mesi consecutivi, stesso rating e regione, identificatori coerenti e manifest FSI verificati.

## Profilo anagrafico e triplo Elo

Il manutentore crea un indice locale in `durable/identity/profile-indexes/`, con puntatore atomico `durable/identity/profiles-current.json`. La costruzione scansiona **integralmente** lo ZIP FIDE combined già autorizzato e memorizza i soli 28.764 FIDE ID collegati ai VERIFIED correnti, conservando Standard/Rapid/Blitz, nome, federazione e titolo FIDE. Per i giocatori nei roster FSI ufficiali Toscana/17029 aggiunge cognome, nome, categoria, anno nascita e codice di nazionalità FSI. Per gli altri conserva il profilo del registro nazionale come fonte operativa, esplicitamente segnalata.

Non si inferisce data di nascita completa da un anno, né nazionalità da una federazione FIDE. Se la lista FIDE contiene rating zero, l'Elo è `null` (non classificato). Per incongruenze di anno nascita, non assegna rating. Ogni indice ha fingerprint degli input, SHA256 e provenienza, rimane privato e non pubblicabile. L'import FIDE di mesi futuri resta soggetto al consenso sull'archivio locale.

```bash
antiskill stats identity --fsi 192691
antiskill stats identity --fide 28551931 --format json
python3 scripts/build_stats_player_profiles.py --root /srv/archive/chess/antiskill
```
