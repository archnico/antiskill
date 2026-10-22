# ANTISKILL THEORY — SPECIFICA V1

Stato: CONGELATA  
Corpus iniziale: corpus di riferimento congelato (nome del file non pubblicato)

## 1. Obiettivo

Misurare, per fascia Elo:

1. quante mosse giocate appartengono alla teoria storicamente attestata;
2. con quale frequenza viene scelta una mossa rara o mai giocata prima;
3. a quale ply e a quale numero di mossa la partita esce dalla teoria;
4. quanto varia il risultato cambiando la soglia minima richiesta per definire una mossa teorica.

L'esperimento misura il comportamento rispetto a un libro storico, non la qualità
oggettiva delle mosse secondo Stockfish.

## 2. Separazione temporale

### Libro storico

Tutte le partite con anno valido non successivo al 2023.

### Campione di test

Tutte le partite datate 2024.

Le partite del 2024 non vengono mai aggiunte al libro durante l'analisi.

Questo impedisce:

- contaminazione con dati futuri;
- apprendimento tra partite appartenenti al campione di test;
- dipendenza dall'ordine non cronologico del corpus.

Sono sufficienti anno e partita PGN valida. Le date 2024 con mese o giorno
sconosciuto possono essere incluse.

## 3. Partite incluse

### Libro storico

Sono incluse le partite che:

- hanno un anno valido non successivo al 2023;
- iniziano dalla posizione iniziale standard;
- hanno una mainline PGN analizzabile.

Il rating non è obbligatorio nel libro storico.

### Campione 2024

Sono incluse le partite che:

- hanno anno 2024;
- iniziano dalla posizione iniziale standard;
- hanno una mainline PGN analizzabile;
- contengono almeno un rating individuale valido.

Un rating è valido se è un intero compreso tra 100 e 4000.

Se è presente soltanto `WhiteElo`, vengono analizzate soltanto le decisioni del
Bianco. Lo stesso vale simmetricamente per `BlackElo`.

## 4. Esclusioni

Sono escluse:

- partite con `SetUp "1"` o posizione FEN non standard;
- varianti diverse dagli scacchi standard;
- partite prive di mainline valida;
- decisioni del giocatore privo di rating valido;
- mosse successive alla ply 60 per le metriche principali.

La ply 60 corrisponde alla conclusione della trentesima mossa del Nero.

Commenti, NAG, diagrammi e varianti laterali vengono ignorati. Viene analizzata
soltanto la mainline.

`TimeControl` non è usato come filtro, perché nel campione di riferimento 2024 è presente
soltanto nel 7,31% delle partite.

## 5. Deduplicazione

Libro e campione di test vengono deduplicati separatamente.

L'impronta di una partita comprende:

- posizione iniziale;
- sequenza completa delle mosse mainline in formato normalizzato;
- data;
- nomi normalizzati dei giocatori;
- risultato.

All'interno di una singola partita, la stessa coppia posizione-mossa può
contribuire al conteggio del libro al massimo una volta.

Anche una posizione ripetuta nella stessa partita contribuisce al conteggio
della posizione al massimo una volta.

## 6. Identità di una posizione

Due posizioni sono considerate uguali quando coincidono:

- disposizione dei pezzi;
- colore al tratto;
- diritti di arrocco effettivi;
- possibilità effettiva di cattura en passant.

Sono ignorati:

- contatore delle cinquanta mosse;
- numero progressivo della mossa.

La chiave deve quindi essere sensibile alle trasposizioni ma non ai contatori
irrilevanti per le mosse legali disponibili.

## 7. Conteggi del libro

Per ogni posizione vengono registrati:

- numero di partite storiche distinte in cui compare;
- elenco delle mosse giocate da quella posizione;
- numero di partite storiche distinte in cui compare ciascuna mossa.

I conteggi indicano precedenti in partite distinte, non il numero grezzo di
occorrenze dentro il PGN.

## 8. Classificazione primaria

La soglia primaria è `T = 5`.

Per una decisione del campione 2024:

### Posizione nota

La posizione compare in almeno 5 partite del libro.

### Mossa teorica consolidata

La posizione è nota e la mossa compare in almeno 5 partite del libro.

### Precedente raro

La posizione è nota e la mossa compare da 1 a 4 volte.

### Novità stretta

La posizione è nota e la mossa non compare mai nel libro.

### Libro esaurito

La posizione compare in meno di 5 partite storiche.

Una posizione sconosciuta non trasforma automaticamente la mossa in una
novità: viene classificata come `book_exhausted`.

## 9. Analisi di sensibilità

Tutte le metriche principali vengono ripetute con:

- `T = 1`
- `T = 3`
- `T = 5`
- `T = 10`

Per ogni soglia T:

- posizione nota se compare almeno T volte;
- mossa teorica se compare almeno T volte.

`T = 5` rimane il risultato principale.

## 10. Metriche per decisione

Per ogni fascia Elo vengono calcolati:

- numero totale di decisioni con rating valido;
- numero e percentuale di posizioni note;
- numero e percentuale di mosse teoriche consolidate;
- numero e percentuale di precedenti rari;
- numero e percentuale di novità strette;
- numero e percentuale di posizioni con libro esaurito.

Vengono mostrate due percentuali differenti:

### Copertura del libro

Posizioni note diviso tutte le decisioni con rating valido.

### Aderenza alla teoria

Mosse teoriche diviso decisioni effettuate da posizioni note.

La percentuale di novità usa come denominatore le decisioni da posizioni note.

## 11. Prima uscita dalla teoria

La partita viene percorsa dalla prima mossa mantenendo uno stato di
`linea ancora in teoria`.

La prima uscita può essere:

### Deviazione del giocatore

La posizione è nota, ma la mossa è un precedente raro o una novità stretta.

Vengono registrati:

- ply;
- numero della mossa;
- colore;
- rating del giocatore;
- fascia Elo;
- tipo di uscita;
- frequenza storica della mossa;
- frequenza storica della posizione.

### Esaurimento del libro

La posizione stessa non raggiunge la soglia minima.

Questo caso viene registrato separatamente e non viene attribuito come novità
al giocatore.

Dopo la prima uscita non vengono cercate ulteriori prime uscite nella stessa
partita.

## 12. Probabilità di uscita

Per ogni fascia Elo viene calcolata anche:

`uscite causate dalla fascia / turni giocati dalla fascia mentre la linea era ancora in teoria`

Questa misura evita di confrontare soltanto il numero assoluto di uscite tra
fasce che hanno quantità diverse di partite e opportunità di deviazione.

## 13. Profondità teorica

Per ciascuna fascia Elo del giocatore che causa la prima deviazione vengono
calcolati:

- ply media;
- ply mediana;
- primo e terzo quartile;
- numero medio della mossa;
- numero mediano della mossa;
- distribuzione completa delle uscite.

La sopravvivenza della linea teorica viene misurata almeno alle ply:

- 10
- 20
- 30
- 40
- 50
- 60

## 14. Fasce Elo

- `under_1200`
- `1200_1399`
- `1400_1599`
- `1600_1799`
- `1800_1999`
- `2000_2199`
- `2200_2399`
- `2400_plus`

Bianco e Nero vengono analizzati:

- insieme;
- separatamente.

## 15. Validazione manuale

Il programma deve estrarre un campione riproducibile contenente almeno:

- 100 mosse teoriche;
- 100 precedenti rari;
- 100 novità strette;
- 100 casi di libro esaurito.

Per ogni caso devono essere salvati:

- intestazioni della partita;
- FEN normalizzata;
- mossa giocata;
- conteggio storico della posizione;
- conteggio storico della mossa;
- soglia applicata;
- fascia Elo;
- ply.

Il campione usa un seed fisso documentato.

## 16. Output previsti

Directory:

`results/theory/reference-v1/`

File principali:

- `summary.txt`
- `run-metadata.json`
- `decision-by-rating.csv`
- `first-exit-by-rating.csv`
- `first-exit-distribution.csv`
- `book-survival.csv`
- `threshold-sensitivity.csv`
- `color-comparison.csv`
- `validation-sample.csv`
- `errors.csv`

## 17. Controlli obbligatori

Il programma deve riportare:

- partite lette;
- partite deduplicate;
- partite storiche valide;
- partite 2024 valide;
- partite escluse per ciascun motivo;
- errori PGN;
- mosse illegali;
- decisioni con e senza rating;
- numero di posizioni uniche nel libro;
- memoria massima approssimativa;
- tempo di costruzione e analisi;
- versione del programma;
- parametri e seed utilizzati.

## 18. Risultati dell'audit preliminare

Corpus di riferimento:

- 13.043.255 partite;
- 12,15 GiB;
- 642.946 partite nel 2024;
- 642.805 date 2024 complete;
- 586.767 partite 2024 con entrambi i rating;
- 35.297 con un solo rating;
- 20.882 senza rating;
- 790 partite 2024 da posizione non standard.

Le 7.773.309 righe che iniziano con `[` ma non sono tag PGN si trovano tutte
nel movetext o nei commenti. Non è stata trovata alcuna intestazione
malformata.

## 19. Interpretazione

In questa specifica, “teoria” significa:

> una mossa sufficientemente attestata nel corpus storico precedente al
> campione di test.

Non significa necessariamente:

- migliore mossa del motore;
- mossa raccomandata dai manuali;
- mossa conosciuta coscientemente dal giocatore;
- prova diretta di preparazione mnemonica.

Il risultato misura la compatibilità delle scelte del giocatore con la teoria
storicamente osservata.
