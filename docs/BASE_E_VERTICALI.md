# Base comune e verticali

Antiskill si può leggere con due mappe sovrapposte.

- Le **aree** (`Nucleo`, `Fonti`, `Analisi`, `Rapporti`...) assegnano responsabilità e dipendenze.
- La **Base comune** raccoglie le capacità stabili che più usi devono poter condividere.
- Le **verticali** attraversano le aree e terminano in un risultato utilizzabile: un rapporto, un dossier, una preparazione, un esperimento o un'integrazione.

Le due mappe non competono. Le aree dicono *chi possiede cosa*; Base e verticali dicono *che cosa stiamo costruendo e per quali usi*.

## Base comune

Il Nucleo è il cuore semantico della Base, ma la Base non coincide con una sola directory o area. Comprende anche contratti stabili di Fonti, Conoscenza, Analisi, Interoperabilità, Piattaforma e Qualità quando sono condivisi da più verticali.

Una capacità appartiene alla Base quando è generale, verificabile e ragionevolmente riusabile da almeno due usi indipendenti. Oggi la Base tende a comprendere:

1. **custodia dell'evidenza** — byte originali dove promesso, snapshot, hash, provenienza;
2. **identità e deduplica** — occorrenze, partite logiche, giocatori, eventi, posizioni e relazioni senza merge distruttivi;
3. **interrogazione e indici** — query strutturate su tag, persone, eventi, date, posizioni e trasposizioni;
4. **riconciliazione di Fonti e Conoscenza** — FIDE, FSI, sistemi torneo, broadcast, rating list, albi e altre autorità con provenance propria;
5. **corpus arricchito** — asserzioni, annotazioni e valutazioni ricevute dalle fonti restano livelli attribuiti e interrogabili, senza essere fuse nel raw;
6. **analisi riusabile** — riuso preferenziale di analisi affidabili già disponibili e, quando serve, Stockfish, Syzygy e modelli umani identificati da soggetto, build/pesi e profilo;
7. **esecuzione affidabile** — job, manifest, checkpoint, resume, cache verificata e distribuzione opzionale;
8. **contratti macchina** — API interne strutturate, CLI/JSON/JSONL/PGN versionati, senza parsing dell'output umano;
9. **qualità operativa** — test, benchmark, portabilità, privacy, diritti, fail-closed e pubblicabilità.

La Base cresce per capacità condivise, non inglobando ogni funzione possibile nel Nucleo. Il contratto trasversale dei dati è descritto in [Corpus verificato e arricchito](CORPUS_VERIFICATO.md).

## Verticali

Una verticale parte da una domanda concreta e riusa la Base end-to-end. Il suo vertice deve essere qualcosa che una persona o un altro programma può usare direttamente.

Esempi già individuati:

| Verticale | Vertice utilizzabile | Stato relativo |
|---|---|---|
| **Ricerca effetto Antiskill** | rapporto scientifico riproducibile | avanzata |
| **Giocatore / scouting** | dossier giocatore o avversario | avanzata / in consolidamento |
| **Repertorio / preparazione** | rapporto di repertorio e linee rilevanti | avanzata sul lato dati, prodotto da consolidare |
| **Performance / qualità** | rapporto su risultati, errori, occasioni e partite notevoli | parziale |
| **Torneo / popolazioni** | rapporto di torneo, copertura e confronti | specifica avviata |
| **Identità scacchistica** | profilo riconciliato fra fonti e account | infrastruttura in sviluppo |
| **Storia / record** | record e ricostruzioni verificabili | avviata sul lato Fonti/Conoscenza |
| **Didattica** | selezione di posizioni ed esempi per studio | esplorativa |
| **Confronto ecosistemi** | confronto riproducibile OTB/Lichess/Chess.com o altre fonti | esplorativa |

Questo elenco non è chiuso e gli stati cambiano. Serve a verificare se la Base sta davvero servendo usi diversi.

## Regola di sviluppo

Quando una verticale ha bisogno di una nuova capacità:

1. verificare se la capacità è specifica del prodotto o generale;
2. se è generale, darle un contratto nella Base e provarla con almeno due consumatori quando possibile;
3. se è specifica, mantenerla nella verticale senza duplicare identità, ingestione, deduplica o provenance;
4. non creare pipeline ombra che reimplementino il Nucleo per ottenere più rapidamente un singolo rapporto;
5. promuovere in Base solo ciò che ha semantica chiara, test e costo di manutenzione giustificato.

Una verticale può sperimentare rapidamente in Laboratorio, ma il successo dell'esperimento non rende automaticamente stabile la sua infrastruttura privata.

## Come misuriamo la maturità della Base

La prova più utile non è il numero di moduli. È riuscire a portare al vertice verticali molto diverse usando gli stessi contratti.

Il checkpoint desiderato è produrre almeno quattro famiglie di risultati — per esempio **Ricerca Antiskill, Giocatore, Repertorio e Corpus/Torneo** — senza quattro pipeline indipendenti per:

- acquisire e identificare partite;
- riconciliare identità;
- cercare posizioni o tag;
- custodire provenienza;
- lanciare lavori lunghi;
- riusare analisi;
- generare output strutturati.

Quando una correzione nella Base migliora più verticali senza cambiarne il significato, la separazione sta funzionando.

## Conseguenze pratiche

Le priorità comuni sono quindi:

- rendere davvero interattive le query della Base;
- estendere gli indici oltre i pochi tag iniziali, incluse le posizioni;
- consolidare identità e joins fra fonti;
- definire Job/Run e resume come primitive comuni;
- rendere le analisi costose riusabili e tracciabili;
- far consumare ai Rapporti API strutturate comuni;
- usare le verticali più mature come acceptance end-to-end della Base.

La Ricerca scientifica Antiskill resta una verticale privilegiata perché stressa provenance, campionamento, analisi e riproducibilità, ma non possiede la semantica della Base.
