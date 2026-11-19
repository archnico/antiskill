# Antiskill Rankings · Top 3, tesseramenti e statistiche circoli

**Stato:** verticale locale eseguibile, dati non certificati per pubblicazione automatica. Niente pubblicazione diretta di elenchi di minori.

## Fonti, precedenza, periodo

1. **FSI tesserati (autorità per affiliazioni):** il nuovo connettore sources sync fsi-members importa un CSV ufficiale senza intestazione (15 colonne). Richiede anno e un filtro obbligatorio per circolo o regione di affiliazione. Salva snapshot e manifest con BLAKE3. Il CSV fornisce ID FSI, nome, anno di nascita, società, province, regione della società e tipo tessera, ma non l'ID FIDE.
2. **FIDE rating lists (autorità per rating Standard/Rapid/Blitz):** parser XML in streaming che conserva in memoria soltanto gli ID FIDE necessari. Il file XML oppure ZIP è fornito dall'operatore se ne ha il diritto; Antiskill non esegue download né redistribuisce la banca dati FIDE senza autorizzazione.
3. **Registro nazionale identità (verifiche FSI–FIDE):** con `--verified-links` il motore usa esclusivamente crosswalk con stato `VERIFIED`, esportato in lettura da SQLite con `scripts/export_verified_links.py`. I collegamenti `REVIEW` e `SOURCE_ASSERTED` non diventano rating. Conflitti fra Allin e registro vengono esclusi e conteggiati.
4. **Torneionline Allin:** conserva il collegamento operativo storico fra ID FSI e FIDE, soltanto quando non è fornito il registro verificato. Non decide lo stato del tesseramento o il rating FIDE. Il CSV Allin era ancora fermo ad aprile 2026 nel controllo del 19 novembre 2026.

Con ID ambigui o anno di nascita discordante non viene attribuito il rating. Un giocatore presente in FSI ma non in Allin resta nel censimento, anche se non può essere abbinato a un Elo. Quando viene fornito l'XML FIDE, un rating mancante non ricade su quello obsoleto di Allin.

## Comandi essenziali

    antiskill sources sync fsi-members --root /archivio --year 2026 --club 17029
    antiskill sources sync fsi-members --root /archivio --year 2026 --region TOS
    antiskill sources import fide-local --root /archivio --file /dati/standard_oct26frl_xml.zip --month 2026-10 --kind standard --url https://ratings.fide.com/download/standard_oct26frl_xml.zip

    python3 scripts/export_verified_links.py --db /archivio/identity_registry.sqlite3 --output /dati/verified-links.csv

    antiskill rankings club --snapshot /dati/allin.csv --month 2026-11 --club 17029 \
      --fsi-csv /dati/fsi-club.csv --fsi-as-of 2026-11-19 --rating standard

    antiskill rankings top3 --snapshot /dati/allin.csv --month 2026-11 --region TOS \
      --fsi-csv /dati/fsi-regione.csv --fsi-as-of 2026-11-19 \
      --fsi-manifest /dati/fsi-manifest.json \
      --verified-links /dati/verified-links.csv \
      --fide-xml /dati/fide-2026-11.xml --fide-month 2026-11 \
      --previous-fide-xml /dati/fide-standard-2026-10.xml.zip --previous-fide-month 2026-10 \
      --rating standard --require-official --output /rapporti/novembre.json

    # Per Rapid ottobre standalone, il rating è nel campo XML generico <rating>.
    antiskill rankings club --snapshot /dati/allin.csv --month 2026-11 --club 17029 \
      --rating rapid --under 18 --fsi-csv /dati/fsi-club.csv --fsi-as-of 2026-11-19 \
      --fsi-manifest /dati/fsi-club-manifest.json --verified-links /dati/verified-links.csv \
      --fide-xml /dati/fide-nov-combined.zip --fide-month 2026-11 \
      --previous-fide-xml /dati/rapid_oct26frl_xml.zip --previous-fide-month 2026-10 \
      --previous-fide-kind rapid --require-official --output /rapporti/rapid-u18.json

Il flag --require-official obbliga a fornire FSI+manifest verificabile+FIDE e un rating internazionale; non significa che l'autenticità delle fonti o i diritti siano automaticamente certificati.

## Report

- Top 3: dodici gruppi U08–U18, assoluta e femminile; rating decrescente, poi nome/ID come spareggio tecnico. U08 comprende anche i bambini più piccoli.
- Circolo: società federale con filtri Standard/Rapid/Blitz, età giovanile e limite opzionale.
- JSON antiskill.rankings.v1 oppure CSV, con impronte BLAKE3 di tutte le fonti, dichiarazioni del mese FIDE e acquisizione FSI, etichette delle autorità e campo certified sempre false.
- Coverage FSI: totale tesserati, collegati ad Allin, disponibili su FIDE, rating presenti, tipologie tessera, conteggi per provincia del circolo e residenza, circoli distinti. Se si usa `--verified-links`, sono visibili `registry_verified_in_scope`, `registry_conflicts` e `registry_nonverified_rows` (quest'ultimo è sull'intero CSV del registro, non solo sulla regione).
- Rating zero e 1399 esclusi come convenzione pilota da certificare con le regole federali vigenti.

## Limitazioni intenzionali

- --fsi-as-of e --fide-month sono dichiarati dall'operatore: senza controllo del manifest e della release non sono prova sufficiente della freschezza. La data di acquisizione FSI è registrata nel manifest: quando fornito via --fsi-manifest, il programma verifica scope, URL, HTTP 200, BLAKE3, dimensione e congruenza del giorno.
- Con due liste FIDE ufficiali di mesi consecutivi (XML/ZIP), `rating_delta` calcola la variazione Elo tra quelle liste per FIDE ID. Per le liste FIDE storiche separate Rapid/Blitz, specificare `--previous-fide-kind rapid|blitz`: quelle liste usano `<rating>` anziché `<rapid_rating>`/`<blitz_rating>`. Senza lista FIDE precedente il delta rimane null. `previous_rank`, `rank_change` ed `entered_top` restano null: per attribuirli servono anche roster FSI storici completi. I confronti Allin storici restano operativi ma non ufficiali.
- La lista FSI CSV non contiene il sesso; la classifica femminile può usare il dato FIDE, altrimenti il ponte Allin, con copertura incompleta esplicitata dal report.
- Per il Top 3 regionale una esportazione limitata a un singolo club è rifiutata; per un report completo deve essere preservato l'export regionale integrale.
- Non distribuire né pubblicare dati grezzi FIDE/FSI senza verificare termini d'uso, riservatezza e diritti dei minori.

## Debiti di maturità

1. Riconciliare manifest FSI verificati e ID release FIDE nel report, con controllo integrità/freschezza.
2. Confrontare tre mesi storici con i report canonici, inclusi pari Elo, criteri U08, categoria e società.
3. Supportare confronto di posizione e ingressi con due roster FSI ufficiali completi; il solo delta Elo FIDE tra due mesi è già disponibile.
4. Migliorare la copertura degli identificativi senza deduplicazioni distruttive. Prima validazione 19/11/2026: Toscana 1.499 FSI, 691 link Allin e 680 link `VERIFIED` dal registro, 11 in revisione (nessuno nei Top 3 Standard del collaudo).
5. Approvazione editoriale preventiva alla pubblicazione di classifiche dei minori.

**Garanzia:** calcolo locale deterministico e fonti separatamente attribuite, non classifica federale già certificata.
