# Antiskill Rankings · verticale v1

**Stato:** MVP locale da certificare sulle serie storiche. Non pubblica automaticamente dati di tesserati, classifiche o report.

## Perché una verticale

Una singola Base conserva evidenza, provenienza e identità; Rankings calcola viste riproducibili per regione, circolo e fascia giovanile. Non crea una pipeline parallela per acquisire Allin: riusa `antiskill sources sync torneionline-allin --root DIR`, che produce oggetti immutabili content-addressed. La sorgente v1 rimane un CSV Allin fornito esplicitamente dall'operatore.

## Esempi

```bash
antiskill rankings top3 --snapshot /dati/allin_2026-11.csv --month 2026-11 --region TOS
antiskill rankings top3 --snapshot /dati/allin_2026-11.csv --month 2026-11 \
  --previous /dati/allin_2026-10.csv --previous-month 2026-10 --format csv
antiskill rankings club --snapshot /dati/allin_2026-11.csv --month 2026-11 \
  --club 17029 --rating standard
antiskill rankings club --snapshot /dati/allin_2026-11.csv --month 2026-11 \
  --club 17029 --rating rapid --under 18 --output /dati/report-rapid-u18.json
```

## Regole

- `top3`: 12 gruppi U08/U10/U12/U14/U16/U18 × assoluta/femminile; ogni gruppo max 3 per default, ordinato Elo decrescente e poi nominativo/ID per risolvere i pari rating in modo **tecnico**, non sportivo. U08 include anche bambini più giovani della soglia.
- `club`: vista di tutti i tesserati del codice società indicato (nessun limite di posizioni); opzionalmente filtro `--under 18`, rating Standard o Rapid.
- Regione = **Regione Tesseramento**, non Regione Residenza; società = **Circolo Attuale**; è richiesto `Anno Ultimo Tesseramento` uguale all'anno del periodo richiesto. Nessun aggiornamento viene dedotto dalla data di esecuzione.
- Rating 0 (non attribuito) e 1399 (sentinella nei workflow storici) non entrano in classifica. Le soglie e i criteri federali vigenti richiedono una revisione prima della certificazione.
- La variazione Elo è calcolata sull'**intero snapshot precedente**, anche se il giocatore era fuori Top 3. Prima si risolve l'identità con ID FSI e FIDE; il nome completo + nascita + sesso è fallback solo se univoco. Identità ambigue producono delta null, mai confronti indovinati.
- Variazione posizione: confronto della stessa classifica, ricalcolata sullo snapshot precedente. `entered_top` distingue gli ingressi in classifica. Senza fonte storica precedente, delta e ingressi restano null.
- Mesi precedenti soltanto consecutivi in v1. Un file precedente uguale al corrente è rifiutato. `--month` è dichiarato dall'operatore: va riconciliato alla data di pubblicazione reale del dataset.
- JSON: contratto `antiskill.rankings.v1`; CSV: tabella piatta. Impronte BLAKE3 dei file sorgente presenti nell'output, percorso locale non esposto (soltanto filename).
- Output `--output` mai sovrascritto, niente invii esterni, niente dati personali nel repository.

## Limiti e debiti prima della certificazione pubblica

1. Integrare snapshot mensili **ufficiali FIDE** per Elo Standard/Rapid/Blitz e FSI per affiliazioni e tesseramento. I rating in Allin sono una copia operativa, non la fonte autoritativa.
2. Certificare sui report storici pubblicati e correggere eventuali differenze di regole U8, pari merito, nazionalità, disponibilità rating, anno sportivo e 1399.
3. Esporre `snapshot_id` dei manifest acquisizione e implementare import diretto della Base con controlli di copertura, freschezza e anomalie.
4. Separare pubblicazione editoriale da dati interni: privacy dei minori, verifica diritti, consenso/criteri di pubblicazione, doppia approvazione per output pubblico.
5. Aggiungere serie storiche, classifiche social e spiegazione delle variazioni a partire dai tornei, solo dopo controllo risultati ufficiali.

**Garanzia attuale:** motore locale deterministico testato, non graduatoria federale certificata.
