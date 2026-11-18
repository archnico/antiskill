# Stats Italia — tesseramenti territoriali (specifica v0.1)

Stato: **proposta di verticale, non comando implementato**. Tracciamento: [#20](https://github.com/archnico/antiskill/issues/20). Dipendenze: [#19](https://github.com/archnico/antiskill/issues/19) (identità italiana), [#18](https://github.com/archnico/antiskill/issues/18) (freschezza ALLIN), [#13](https://github.com/archnico/antiskill/issues/13) (ricette).

## Risultato per l'utente

> Quanti tesserati ha la provincia di Firenze nel 2026? Quanti per tessera agonistica, ordinaria o junior? Come è cambiato il numero dal 2023? Quali circoli crescono? Quanti giovani nuovi entrano e quanti non rinnovano?

Un rapporto **senza Stockfish**, interrogabile per anno/stagione, Italia → regione → provincia → circolo, tipo tessera e ulteriori filtri quando la copertura li consente. Ogni numero deve poter mostrare la propria definizione, provenienza, data di riferimento e grado di completezza.

Questa verticale riusa la Base per snapshot, identificazione e riconciliazione; **non** costruisce un database identità in parallelo. La specializzazione FSI appartiene a Fonti, le aggregazioni a Rapporti e i contratti riutilizzabili alla Base.

## Grana, geografia e dimensioni

Un fatto elementare è una **asserzione di tesseramento** dichiarata da una fonte: `(source_key, snapshot_id, season, fsi_id?, club_code?, card_code?, card_number?, observed_at, valid_from?, valid_to?)`. Conservare il raw e le asserzioni senza fusioni distruttive. `fsi_id` è identificatore della persona nella fonte, `club_code` identifica il circolo nella stessa fonte; i nomi possono cambiare.

La pagina ufficiale FSI dei tesserati espone **due geografie** da non confondere:

1. `player_area`: provincia/regione dichiarata per il giocatore nella fonte; non assumerla residenza anagrafica se la fonte non la certifica.
2. `club_area`: provincia/regione della società a cui risulta collegata la tessera.

Le query richiedono `axis=player|club`. Anche nei rapporti nazionali l'asse resta nei metadati. Geografia assente = `unknown`, non geocodifica dedotta da nome/cognome. Conservare codici raw, codici normalizzati e versione della mappa territoriale; distinguere **regioni amministrative** da eventuali circoscrizioni/comitati FSI non perfettamente sovrapponibili.

Dimensioni combinabili: `season`, `as_of`, `axis`, `level=nation|region|province|club`, `card_code`, `card_family`, fascia d'età alla data della stagione, genere dichiarato, categoria FSI, circolo, stato nuovo/rinnovo quando verificato. Rating e attività torneo sono join temporali opzionali e attribuiti alla loro fonte, **non** campi intrinseci della tessera.

## Vocabolario tessere FSI 2026

Fonte ufficiale: [Normativa affiliazione e tesseramento FSI 2026](https://federscacchi.it/doc_vari/MNOR_Normativa_2026.pdf), pag. 7.

| Codice | Tipo | Famiglia di rapporto |
|---|---|---|
| TA | Agonistica | agonistica |
| TU | Agonistica ridotta | agonistica |
| TD | Agonistica per detenuti | agonistica; **dato sensibile: attenzione alle piccole celle** |
| TO | Ordinaria | ordinaria |
| TR | Ordinaria ridotta | ordinaria |
| TJ | Junior | junior |
| TG | Junior ridotta | junior |
| TT | Junior agonistica | junior |
| IT | Integrazione da TO a TA | **operazione, non ulteriore persona/tessera indipendente** |

Questo è un **dizionario 2026**, non la garanzia che ogni anno usi le stesse regole. `card_code` raw va preservato; un codice ignoto produce `unmapped`, non classificazione immaginata. Istruttori, arbitri e dirigenti costituiscono **ruoli/tesseramenti ulteriori**, potenzialmente sovrapposti alla qualità di giocatore: mai sommarli automaticamente al totale giocatori.

## Misure non intercambiabili

| Nome | Definizione |
|---|---|
| `registered_people` | Numero di **ID FSI distinti** con tesseramento valido e verificato nello snapshot/periodo selezionato. Un ID mancante resta nel bucket `unresolved`, non viene deduplicato per nome. |
| `membership_relations` | Numero di relazioni distinte persona–circolo–stagione osservate con la relativa provenienza. Un trasferimento può produrne più di una. |
| `observed_rows` | Righe raw osservate nella fonte, anche se duplicate; metrica diagnostica, non tesserati. |
| `cards_issued` | Solo se i record sorgente e la semantica delle emissioni lo consentono; gli upgrade/rimpiazzi non diventano persone aggiuntive. Altrimenti `null` con motivo `not_observable`. |
| `new_first_time` | Solo con indicatore di prima iscrizione attestato dalla fonte o storia personale sufficientemente completa; assenza in archivio locale ≠ prima tessera in assoluto. |
| `renewed`, `not_renewed` | Confronto tra ID FSI su due stagioni **complete e comparabili**; attenzione ai tesserati tardivi, trasferimenti e differenze tra date di acquisizione. |
| `year_over_year_pct` | `(current - previous) / previous × 100` solo con denominatore positivo, definizione identica e coverage adeguata; altrimenti `null`. |

Una persona con due circoli nella stessa stagione vale **una volta a livello Italia**. In una vista `ever_during_season` può comparire in due gruppi locali: la somma delle province/circoli **non** deve essere presentata come identica al totale nazionale distinto. In una vista `as_of` si usa soltanto un'appartenenza valida alla data **se le date di validità esistono**. Un semplice snapshot non è prova di tutte le date storiche; in tal caso usare esplicitamente `observed_at_snapshot` e non inventare una cronologia.

Ugualmente, la conversione **TO → TA** lascia una sola persona, con storico dei tipi e, quando attestato, tipo finale alla data di taglio.

## Fonti, freschezza, qualità

**Primarie:** elenco [tesserati FSI](https://www.federscacchi.com/fsi/index.php/struttura/tesserati) (CSV ufficiale quando disponibile), elenco società/affiliazioni FSI. **Secondarie:** `torneionline-allin` per collegamenti ID e dati storici; FIDE per rating e ID internazionale, mai per concludere che una tessera FSI sia attiva.

Il dump ALLIN individuato nel pilota di [#19](https://github.com/archnico/antiskill/issues/19) era datato **28 aprile 2026**: non è fonte sufficiente a presentare **totali correnti 2026**. I numeri pubblicati come «totale Italia» richiedono una copertura FSI comprovata: totale righe attese, paginazione/segmentazione eventualmente completata, filtri e limiti, data del dataset e confronto con aggregati federali quando disponibili.

Ogni output conserva `source_key`, `snapshot_id`, `fetched_at`, `source_published_at?`, hash raw, versione parser, licenza/diritti, `coverage_status=complete|partial|unknown`, e motivi di non-comparabilità. Nessuna sostituzione silenziosa di `unknown` con zero.

I dati importati non diventano automaticamente pubblicabili. Per vista pubblica usare aggregati con protezione delle celle a rischio identificazione (particolarmente per minori e tipologie sensibili), evitando di replicare liste nominative.

## Contratto di interrogazione proposto

Non ancora implementato; sintassi da riconciliare con il registro ricette di #13:

```text
antiskill report memberships --season 2026 --axis club --level province --group-by card_family --as-of 2026-11-19 --format json
antiskill report memberships --season 2026 --axis player --level region --card-code TT --format csv
antiskill report memberships --from-season 2023 --to-season 2026 --axis club --level club --metric registered_people
```

Schema concettuale `antiskill.memberships.territorial.v1`: `recipe`, `version`, `scope` (season/date/axis/grouping/filters/assignment_policy), `definitions`, `totals`, `groups[]`, `unknowns`, `coverage`, `sources[]`, `warnings[]`, `generated_at`. I gruppi contengono numeri, non liste di persone per default. Output deterministico per lo stesso snapshot e gli stessi parametri, indipendentemente dal momento di esecuzione (eccetto `generated_at`, separato dall'impronta del risultato).

## Fixture sintetica minima e risultati attesi

I seguenti ID sono inventati, non persone reali. Stagione 2026, vista *ultimo tesseramento esplicitamente osservato* (non cronologia ipotizzata):

- FSI 1001, giocatore PT, circolo 17001/FI, **TA**; la medesima riga arriva da due copie dello stesso export.
- FSI 1002, giocatore FI, circolo 17001/FI, **TO** poi **IT → TA**, con prova del passaggio.
- FSI 1003, giocatore BO, prima circolo 17001/FI poi 5001/BO, **TG**, con date affidabili.
- FSI 1004, giocatore BO, circolo 5001/BO, **TJ**.
- Una riga **TT**, circolo 5001/BO ma ID FSI assente e non risolvibile.

**Oracoli**:
- persone distinte *identificate* nel totale Italia = **4**; record senza ID = **1 unresolved** (non sommare né inventare identità);
- ultime società dichiarate: circolo FI **2** ID, circolo BO **2** ID, più **1 unresolved** in BO;
- per famiglia sulle quattro persone identificate: agonistica **2**, junior **2**, ordinaria **0**; l'upgrade IT non crea una quinta persona;
- la distribuzione per provincia del *giocatore* differisce da quella del *circolo* (PT **1**, FI **1**, BO **2** contro FI **2**, BO **2**);
- l'opzione `ever_during_season` conta FSI 1003 in entrambi i circoli e avverte che somme locali e persone nazionali possono differire;
- lo storico prima/dopo upgrade conserva l'asserzione TO, senza contarla simultaneamente come tessera finale;
- il gruppo `unresolved` mostra **copertura d'identità incompleta**, non una quinta persona certificata.

## Incrementi di implementazione

1. **Contratto + dizionario versionato + fixture:** oracoli di conteggio e privacy, senza dati reali nel repository.
2. **Fonti FSI / società:** import snapshot e asserts tipizzati con provenance, paginazione completa e join territoriale verificato; nessuna rete obbligatoria per Nucleo.
3. **Aggregazione locale:** distinct-ID, membership, due assi geografici, `unknown`, tipo tessera, export JSON/CSV; test di non-additività.
4. **Serie storiche:** comparabilità degli snapshot, rinnovi/ritorni/abbandoni, trasferimenti e raggruppamenti temporali.
5. **Arricchimento opzionale:** età/categoria/Elo/attività, ruoli e albi, indicatori di partecipazione, usando la stessa infrastruttura identità della #19 e fonti con propria autorità.

**Criterio di successo:** una query come «per ogni provincia Toscana, quante persone tesserate, ripartite per tipo nel 2026?» produce numeri ricontabili da snapshot FSI dichiarato, con distinzione asse giocatore/circolo, nessun doppio conteggio e warnings espliciti sulla copertura.
