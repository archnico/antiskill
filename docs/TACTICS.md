# Antiskill Tactics — ricerca e curatela, non un altro allenatore

**Stato:** proposta di verticale didattica, non funzionalità già implementata.

## Principio

Antiskill Tactics è un **motore di ricerca, selezione, verifica e produzione di materiali tattici** su corpus scacchistici verificabili. Non deve duplicare un allenatore di tattica già esistente.

**Lichess è il primo ecosistema da riusare, non un concorrente da ricostruire.** Prima di introdurre una funzione, verificare se Lichess la offre già con qualità sufficiente e con un contratto d'uso compatibile. Lo sviluppo Antiskill è giustificato solo da una lacuna concreta, dimostrata con un caso d'uso.

## Compiti lasciati a Lichess

- Allenamento interattivo, progressione, rating e statistiche dei problemi.
- Catalogo di temi, difficoltà, aperture e problemi tattici già pubblicati.
- Studi, capitoli e modalità Gamebook per la fruizione degli esercizi.
- Interfaccia web/mobile e gestione degli account dei partecipanti.

Non riprodurre un proprio solver, sistema di punteggi, sessioni di allenamento o interfaccia sociale senza evidenza di una necessità non coperta. Non replicare in massa il database dei problemi in una distribuzione Antiskill.

Riferimenti da valutare come fonti e destinazioni:

- [Lichess Puzzle Database](https://database.lichess.org/#puzzles)
- [Allenamento per temi](https://lichess.org/training/themes)
- [Lichess Studies](https://lichess.org/study)
- [API Lichess](https://lichess.org/api) — solo per funzioni documentate e autorizzate

Le capacità di importazione e automazione vanno verificate nell'API e nella UI effettivamente disponibili: **non dare per acquisita la creazione automatica di Gamebook via API**.

## Il problema che rimane aperto

Un istruttore vuole interrogare anche **partite reali non già selezionate come puzzle**, con richieste composizionali molto specifiche, per esempio:

> Trova partite magistrali nella Siciliana Dragone in cui il Nero gioca ...Txc3 come sacrificio di qualità; individua i momenti realmente istruttivi, verifica le migliori difese e prepara dieci esercizi per uno studio Lichess.

Altri esempi: dono greco, doppio sacrificio degli alfieri, sacrificio in h3, tema tattico in una variante rara di apertura. Non bastano tag generici come «sacrifice», «bishop» o «Sicilian».

La differenza da verificare con Lichess non è «abbiamo più puzzle», ma **possiamo trovare, spiegare e confezionare ciò che il catalogo standard non consente di richiedere con precisione**.

## Pipeline candidata

1. **Riuso prima del calcolo.** Cercare inizialmente tra i problemi Lichess già esistenti; registrare ID, rating, temi e legami con la partita quando disponibili. L'eventuale lacuna va misurata, non presunta.
2. **Ricerca strutturale sulle partite.** Filtrare corpus PGN leciti tramite sequenze di mosse, geometria dei pezzi, variazioni di materiale e posizione; riconoscere trasposizioni. L'ECO non sostituisce una verifica della variante.
3. **Validazione selettiva.** Sottoporre i candidati a motori/tablebase soltanto quando necessario; verificare legalità, alternative, difese non banali, correttezza della soluzione ed eventuale unicità **quando esplicitamente richiesta**.
4. **Curatela didattica.** Scartare esercizi con vantaggio già schiacciante o posta in palio trascurabile, mosse quasi indifferenti, difese irrilevanti, duplicati e posizioni con qualità pedagogica insufficiente. Separare il valore estetico dalla correttezza tattica: i criteri devono essere espliciti e rivedibili.
5. **Consegna interoperabile.** Produrre PGN importabili in Studi Lichess, con varianti e metadati necessari, e manifest strutturati (JSON/JSONL). Non inventare annotazioni attribuite a giocatori o autori: distinguere fonti, analisi engine e testo editoriale.

La Base comune possiede già o deve possedere i contratti condivisi di ingestione, identità, posizione, deduplica, provenance, query, analisi e job. La verticale non ne crea versioni parallele. Vedi [Base comune e verticali](BASE_E_VERTICALI.md).

## Vincoli e qualità

- **Fonte e diritti:** conservare provenienza, ID e snapshot delle partite e dei problemi; verificare la licenza specifica di ogni dataset e rispettare i limiti delle API. Non incorporare per default corpus di terzi nel repository pubblico.
- **Identità dei problemi:** distinguere il puzzle Lichess, l'occorrenza in una partita, la posizione e l'esercizio derivato; mantenere i collegamenti senza fonderli.
- **Verifica:** una mossa unica del motore non equivale automaticamente a un esercizio buono. Nei matti esatti, richiedere prova adeguata e non confondere valutazione vinta con matto forzato.
- **Confini:** nessuna dipendenza di rete, motori o Lichess obbligatoria nel Nucleo; adapter e analisi restano opzionali.
- **Pubblicabilità:** esempi sintetici o licenziati, test riproducibili, niente dati privati o credenziali.

## Prova minima prima di sviluppare un prodotto

Confrontare **due richieste concrete** (per esempio dono greco e ...Txc3 nel Dragone) contro i filtri e i dati di Lichess. Annotare cosa è già risolvibile senza codice nuovo, cosa richiede solo un adapter e quale residuo necessita vera ricerca strutturale.

Solo su quel residuo costruire un piccolo prototipo che:

- restituisca partite realmente identificate con query e provenienza verificabili;
- separi i candidati scartati da quelli validati e ne registri il motivo;
- produca una raccolta PGN apribile in Lichess, senza ricostruire l'esperienza di allenamento;
- misuri accuratezza della ricerca, qualità della selezione, riuso di analisi e costo computazionale.

**Criterio di successo:** un istruttore ottiene una raccolta specifica che non avrebbe potuto produrre con facilità con le sole funzioni di Lichess. Se Lichess copre già il caso, Antiskill deve collegarlo o documentarlo, **non riscriverlo**.
