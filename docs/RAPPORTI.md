# Rapporti

## Scopo

I Rapporti trasformano la Base Antiskill in prodotti leggibili senza obbligare l'utente a lanciare analisi costose. Spesso sono il **vertice visibile di una verticale**: consumano contratti comuni di Nucleo, Fonti, Conoscenza e, quando serve, Analisi/Piattaforma senza costruire pipeline ombra. Devono dichiarare la copertura rilevante e distinguere fatti, conflitti, annotazioni di fonte e derivazioni Antiskill.

## Senza engine

Priorità alta:

### Giocatore

- partite e fonti;
- periodo coperto;
- Bianco/Nero;
- risultati;
- avversari;
- eventi;
- rating disponibili;
- repertorio e frequenze;
- cambiamenti nel tempo;
- completezza dei metadati;
- duplicati e conflitti.

### Repertorio

- prime mosse e frequenze;
- strutture e transposizioni;
- scelte per colore;
- risposte ad avversari;
- evoluzione temporale;
- performance empirica.

### Torneo

- partecipanti osservati;
- round e partite disponibili;
- copertura per fonte;
- duplicati;
- risultati discordanti;
- buchi del broadcast;
- qualità dei tag.

### Corpus

- sorgenti e dimensioni;
- occorrenze e partite logiche;
- distribuzione per anno/evento/giocatore;
- FIDE ID e altri identificatori;
- qualità dei tag;
- non comparabili;
- duplicazione;
- conflitti;
- copertura delle fonti;
- copertura di identità, posizioni, analisi, annotazioni e conoscenza storica.

### Fonti

- contributo di ciascuna fonte;
- sovrapposizione;
- unicità;
- campi forniti;
- tasso di errori;
- data dell'ultimo snapshot quando pertinente;
- freschezza rispetto alla cadenza attesa;
- conflitti e criterio della vista canonica.

## Analisi riusabile e motore

Prima di invocare Stockfish locale, una ricetta cerca analisi già disponibili e sufficientemente qualificate nel corpus, incluse valutazioni provenienti da fonti esterne. Il compute locale colma lacune o produce un profilo omogeneo quando il prodotto lo richiede. Quando la ricetta usa `reuse-first`, i suoi vincoli devono poter essere tradotti in un `antiskill_analysis_requirement_v1` esplicito: origini, metodi, diritti, soglie e ordine degli assi di selezione non vengono dedotti da una graduatoria globale implicita.

Rapporti in cui Stockfish locale può produrre molto valore con costo controllato:

### Critiche

Selezionare prima con criteri economici, poi analizzare poche posizioni:

- grandi cambi di valutazione;
- posizioni tattiche;
- occasioni mancate;
- finali critici;
- mosse uniche;
- partite decisive.

### Preparazione

- linee frequenti dell'avversario;
- deviazioni rare;
- posizioni ricorrenti;
- poche analisi profonde sulle diramazioni più rilevanti.

### Revisione

- errori più costosi;
- migliori risorse non giocate;
- posizioni didattiche;
- selezione di esempi annotabili.

## Priorità

Le prime ricette da stabilizzare sono:

- Giocatore;
- Repertorio;
- Torneo;
- Corpus;
- Fonti.

Sono buoni punti di ingresso anche per contributori senza Rust: la parte difficile iniziale è definire domande e output utili.

## Ricette

Ogni rapporto dovrebbe diventare una ricetta versionata con pochi campi:

```text
nome
versione
input
costo
engine: no|opzionale|richiesto
analysis_policy: reuse-first | standardized | local-only
coverage_required / coverage_reported
output
```

Classi iniziali:

- `A`: interrogazione rapida;
- `B`: elaborazione locale moderata;
- `C`: corpus-scale;
- `R`: ricerca scientifica.

Le ricette permettono a CLI, esecutori e strumenti esterni di chiedere lo stesso prodotto senza duplicarne la logica. Due rapporti che hanno bisogno della stessa capacità devono convergere sulla Base, non mantenerne due implementazioni private.
