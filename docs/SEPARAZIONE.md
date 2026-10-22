# Separazione

La regola è: **prima confini e contratti, poi spostamenti di codice**. La [Base comune](BASE_E_VERTICALI.md) è un confine logico di riuso, non una nuova cartella monolitica: ogni capacità resta nell'area che ne possiede la semantica.

## Nucleo

Il Nucleo deve compilare e funzionare senza rete, engine, GUI o orchestratori esterni.

Materiale corrente principale:

- `src/archive/` — superficie prodotto;
- `src/inspector/` — ingestione, verifica, occorrenze, mainline e Collection;
- `src/dedup/` — deduplica;
- `src/object_store.rs` — custodia content-addressed;
- `src/players.rs` — identità giocatore, con dipendenze da separare progressivamente dagli adapter.

## Fonti

`src/sources/` resta fuori dal Nucleo. Gli adapter possiedono rete, formati e peculiarità della fonte e producono evidenza/provenienza verificabile.

## Rapporti

Le ricette consumano API strutturate del Nucleo, non testo pensato per la CLI umana. Per default non richiedono engine.

## Analisi

Stockfish, Syzygy e modelli umani restano opzionali. Il Nucleo può selezionare posizioni o partite senza doverli eseguire.

## Ricerca

`docs/research/` documenta protocolli e risultati pubblicabili. Corpus e run raw possono restare fuori dal repository. La Ricerca non modifica il Nucleo per adattarlo a un esito.

## Interoperabilità

CLI, JSON, JSONL e PGN sono contratti. En Croissant, Scid e altri programmi sono strumenti esterni, non dipendenze del Nucleo.

## Conoscenza

Dati di riferimento e storici hanno provenienza propria e arricchiscono le viste senza riscrivere l'evidenza raw.

## Piattaforma

Job, cache, resume e distribuzione vivono sopra le operazioni. L'esecuzione locale resta il riferimento semantico.

## Laboratorio

`src/lab/` ospita prototipi. Una promozione richiede problema reale, contratto, test e un uso concreto o uno strumento che la consumi.

## Qualità

Test, fixture, benchmark, portabilità, privacy, documentazione e gate pubblico attraversano tutte le aree.

## Ordine dei lavori

1. definire il confine;
2. aggiungere o stabilizzare il contratto;
3. coprire il comportamento con test;
4. spostare il codice solo se lo spostamento semplifica davvero;
5. aggiornare la documentazione pubblica;
6. eliminare compatibilità temporanee quando non servono più.

## Git

Le aree non sono branch permanenti. Ogni lavoro usa un branch temporaneo e focalizzato.

Per nuovi branch preferire i prefissi definiti in [Mappa](MAPPA.md), in particolare `nucleo/` al posto del vecchio `archivio/`.
