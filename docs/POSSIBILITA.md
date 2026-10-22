# Possibilità

Questo documento raccoglie direzioni plausibili per Antiskill. **Non è una roadmap e non crea impegni di implementazione.**

L'obiettivo è lasciare spazio a contributi che dimostrino valore prima di decidere dove debbano vivere. Le idee diventano **verticali** quando producono un uso end-to-end; le capacità che risultano utili a più verticali vengono consolidate nella [Base comune](BASE_E_VERTICALI.md).

## Nucleo e interrogazione

- ricerca per giocatore, evento, data e fonte;
- ricerca per posizione, sequenza e trasposizione;
- deduplica esatta, canonica e semantica controllata;
- cronologie, copertura e qualità dei metadati;
- ricostruzione di archivi personali, societari o storici;
- confronto tra snapshot di un corpus.

## Giocatori e preparazione

- dossier giocatore;
- repertorio e sue variazioni nel tempo;
- avversari ricorrenti;
- preparazione mirata;
- selezione di partite rappresentative;
- posizioni tipiche e transizioni frequenti.

## Tornei e broadcast

- completezza delle partite;
- confronto tra pairing, risultati e PGN disponibili;
- controllo dei round mancanti;
- riconciliazione tra broadcast e sistemi torneo;
- qualità e provenienza delle partite live;
- dossier evento.

## Analisi

- archivio riusabile di analisi Stockfish;
- Syzygy e finali esatti;
- posizioni e momenti critici;
- errori, occasioni mancate e partite istruttive;
- analisi selettiva guidata dai dati invece di brute force su tutto il corpus;
- confronto tra engine o profili di calcolo;
- modelli di scelta umana.

## Didattica

- raccolte di posizioni da studiare;
- esercizi ricavati da partite reali;
- repertori didattici;
- progressione di un allievo nel tempo;
- selezione automatica di esempi istruttivi;
- esportazione verso strumenti di studio.

## Storia e conoscenza

- albi d'oro;
- tornei storici;
- identità di eventi e giocatori;
- liste rating;
- bibliografie;
- aperture e classificazioni;
- collegamento tra partite e fonti documentarie.

## Federazioni e sistemi torneo

- FIDE e FSI;
- altre federazioni nazionali;
- Vesus, Torneionline, Chess-Results e sistemi analoghi;
- corrispondenze tra identità;
- anagrafiche e società;
- calendari e risultati;
- audit delle discrepanze tra fonti.

## Interoperabilità

- En Croissant;
- Scid;
- notebook e workflow Python/R;
- esportazioni PGN/JSON/JSONL;
- API locali;
- strumenti web o desktop esterni;
- integrazioni con pipeline di broadcast o analisi.

## Ricerca scientifica

- effetto Antiskill;
- modelli di scelta umana;
- distribuzione degli errori;
- rapporto tra rating e decisioni;
- studi su tempo, fase della partita o complessità;
- replicazioni indipendenti;
- benchmark tra modelli umani ed engine.

Ricerca usa viste e campioni congelati e resta separata dal Nucleo.

## Qualità dei dati

- rilevamento di anomalie;
- conflitti tra fonti;
- controlli di completezza;
- fixture PGN ostili;
- metriche di copertura;
- verifica di provenienza;
- benchmark di parser e indici.

## Altre idee

Sono benvenute anche proposte fuori da questo elenco. Una nuova applicazione dovrebbe spiegare almeno:

1. quale problema risolve;
2. quali dati usa;
3. quale parte della Base Antiskill riutilizza e quali capacità comuni eventualmente mancano;
4. quale costo aggiunge;
5. quali rischi introduce per privacy, diritti o manutenzione.

## Dove può finire una proposta

Dopo un prototipo o una discussione, una proposta può diventare:

- **Nucleo**, se è fondamentale e generale;
- **Fonti**, se acquisisce dati esterni;
- **Rapporti** o **Analisi**, se è una ricetta riusabile;
- **Interoperabilità**, se collega uno strumento esterno;
- **Laboratorio**, se è promettente ma immatura;
- un repository o plugin esterno;
- oppure non essere integrata.

Non cerchiamo di prevedere tutto in anticipo: cerchiamo confini semplici dopo aver visto evidenza reale.
