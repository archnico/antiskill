# Antiskill Tactics
## Riconoscere le idee scacchistiche

**Una proposta di ricerca aperta**

Antiskill è un progetto open source dedicato alla ricerca e all'analisi di grandi archivi di partite di scacchi. Dopo una prima applicazione ai matti forzati, vorremmo affrontare un problema più difficile:

**Si può insegnare a un software a riconoscere un'idea tattica, e non soltanto una determinata mossa o disposizione dei pezzi?**

### Il risultato che immaginiamo

Vorremmo poter chiedere al programma, attraverso un comando, qualcosa come:

*«Trova le partite fra giocatori di 1800–2200 Elo nelle quali il Nero ha giocato un sacrificio di qualità in c3 nella Siciliana Dragone, anche in varianti secondarie e attraverso trasposizioni.»*

E ottenere direttamente un **PGN di partite complete e autentiche**, da aprire in un qualsiasi programma di scacchi.

La possibilità più affascinante è esplorare linee rare e scoprire combinazioni che nessuno ha mai raccolto in un libro. A partire dalle stesse partite, vorremmo anche poter ricavare **esercizi di tattica per tema e difficoltà**, sempre in formato PGN.

### Alcuni esempi

Sono proposte di casi da esplorare, non un elenco chiuso:

- **Sacrificio di qualità in c3 nel Dragone** — distinguere `...Rxc3` come vero sacrificio da una semplice cattura; riconoscere l'idea anche attraverso ordini di mosse diversi.
- **Dono greco** — riconoscere il tema di `Bxh7+` (o `...Bxh2+`) e distinguerlo da sacrifici simili solo in apparenza.
- **Doppio sacrificio degli alfieri** — riconoscere il rapporto fra due sacrifici che concorrono alla stessa combinazione.
- **Sacrificio in h3** — individuare quando `...Bxh3`, o un altro sacrificio sulla casa, realizza un'idea tattica significativa.
- **Infilata** — distinguere una vera conseguenza tattica dal semplice allineamento geometrico di pezzi.

Per la ricerca nelle partite è essenziale distinguere ciò che **è stato realmente giocato** da ciò che sarebbe stato soltanto possibile secondo un motore.

### La domanda più ampia

Che cosa rende riconoscibile una stessa idea in posizioni differenti? Che cosa distingue una combinazione corretta da una bella combinazione, o un esercizio istruttivo da uno soltanto difficile?

Ci interesserebbe capire **quali parti di questi concetti possano essere definite e riconosciute formalmente**, quali richiedano valutazioni empiriche e dove rimanga un margine inevitabile di interpretazione.

Non abbiamo scelto un metodo e non vogliamo anticipare le risposte. Ci piacerebbe discuterne con persone interessate alla matematica, alla logica, agli algoritmi o alla modellazione del giudizio umano, lasciando aperta la direzione della ricerca.

### Il contesto del progetto

Antiskill può occuparsi dell'infrastruttura: conservazione delle partite originali, provenienza, ricerca, analisi ed esportazione. **Antiskill Tactics è ancora una proposta**, non una funzione già realizzata. L'obiettivo pratico resta semplice: produrre **file PGN locali**, non pubblicare automaticamente su Lichess.

**La domanda che vorremmo condividere è questa:** *si può costruire una matematica delle idee tattiche che ci permetta di ritrovarle, confrontarle e riscoprirle dentro l'intera storia delle partite giocate?*

---

[Il progetto Antiskill](../README.md) · [Base comune e verticali](BASE_E_VERTICALI.md)
