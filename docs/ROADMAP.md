# Piano

Il Piano indica priorità, non chiude il progetto a idee esterne. Le direzioni esplorative stanno in [Possibilità](POSSIBILITA.md).

## Ora

La priorità generale è **densificare la Base comune** e provarla con verticali diverse. Preferiamo una capacità condivisa che sblocca più usi a una scorciatoia privata per un solo rapporto. Il checkpoint architetturale è far arrivare almeno Ricerca Antiskill, Giocatore, Repertorio e Corpus/Torneo a prodotti utilizzabili usando gli stessi contratti di identità, query, provenance, esecuzione e analisi.

Vedi [Base comune e verticali](BASE_E_VERTICALI.md).

### Nucleo

- mantenere ingestione bounded-memory e deduplica non distruttiva;
- rendere interrogabile il corpus come insieme di evidenza, identità, asserzioni, annotazioni e analisi attribuite;
- introdurre metriche di completezza separate per fonti, partite, identità, metadati, posizioni, analisi e conoscenza;
- ridurre la latenza delle query interattive;
- progettare ricerca per posizione e trasposizione senza dipendenza da engine;
- separare meglio identità giocatore e adapter esterni.

### Rapporti

- definire una ricetta versionata semplice;
- costruire i primi rapporti senza engine: giocatore, repertorio, torneo, corpus, fonti.

### Analisi

- definire un archivio riusabile delle analisi;
- importare e indicizzare valutazioni esterne affidabili, a partire da fonti bulk aperte quando disponibili;
- distinguere analisi di fonte, Stockfish Antiskill, Syzygy e modelli umani;
- evitare ricalcoli identici e preferire il riuso prima del compute locale;
- analizzare dopo aver filtrato, non tutto indiscriminatamente.

### Fonti e Conoscenza

- consolidare gli adapter italiani già avviati;
- rendere le cadenze vere Job di aggiornamento con snapshot, diff, freshness e resume comuni;
- rendere semplici nuovi adapter comunitari;
- documentare autorità contestuale, provenienza e diritti;
- collegare albi, rating list, risultati e dati storici senza appiattirne le fonti;
- introdurre annotazioni editoriali o tecniche solo con provenienza e diritti espliciti.

### Ricerca

- mantenere separati esplorazione, conferma e replicazione;
- registrare ogni formulazione con baseline, archivio/input, estimand, risposta dei dati e stato;
- usare viste e campioni congelati prodotti dalla Base, senza semantiche sorgente private;
- sviluppare nuove baseline o definizioni senza riscrivere i risultati delle precedenti;
- pubblicare risultati senza esporre identità non necessarie.

### Interoperabilità

- mantenere stabili CLI e formati strutturati per le macchine;
- sperimentare integrazioni con strumenti reali senza dipendenze inverse nel Nucleo.

### Piattaforma

- definire Job/Run, checkpoint e resume portabili;
- mantenere l'esecuzione locale come riferimento;
- trattare la distribuzione come acceleratore opzionale.

### Qualità

- ampliare fixture PGN ostili;
- mantenere CI Linux/macOS/Windows;
- automatizzare privacy, link e pubblicabilità;
- ridurre documentazione duplicata o obsoleta.

## Dopo

- ricerca per posizione realmente interattiva;
- ricette di Rapporti eseguibili e componibili;
- archivio Analisi condivisibile tra corpus locali;
- adapter esterni mantenuti dalla comunità;
- pacchetti multipiattaforma più completi;
- integrazioni UI mature quando esiste un contratto pulito.

## Non imposto in anticipo

Non fissiamo oggi:

- una GUI ufficiale;
- un database centrale;
- un unico workflow di Analisi;
- un elenco chiuso di applicazioni;
- quali integrazioni esterne debbano per forza essere integrate.

Prima prototipo ed evidenza, poi confine e manutenzione.

## Vincoli

Restano fuori dal Nucleo come requisiti obbligatori:

- rete;
- engine;
- GUI;
- orchestratori esterni;
- credenziali personali;
- corpus privati.
