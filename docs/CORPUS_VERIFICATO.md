# Corpus verificato e arricchito

## Obiettivo

La Base Antiskill non deve limitarsi a conservare molti PGN. Deve poter costruire e interrogare un **corpus molto grande, verificato, aggiornabile e progressivamente arricchito**, nel quale evidenza, identità, fatti di contesto, annotazioni e analisi restano distinguibili e tracciabili.

Il corpus è quindi una base di conoscenza scacchistica locale costruita da fonti che l'utente è autorizzato a usare. I Rapporti e gli esperimenti sono viste e verticali sopra questa base: non ne possiedono i dati e non creano archivi paralleli.

## Strati

Una stessa partita o posizione può avere più strati contemporaneamente:

1. **evidenza** — file, PGN, pagine, export e snapshot originali;
2. **identità e riconciliazione** — partita logica, persone, account, eventi, round, società e identificatori esterni;
3. **asserzioni di fonte** — rating, risultati, classifiche, albi, date, ruoli e altri fatti dichiarati da una fonte;
4. **annotazioni di fonte** — commenti, NAG, varianti, valutazioni e analisi già pubblicate o distribuite da una fonte;
5. **derivazioni Antiskill** — normalizzazioni, collegamenti, indici, metriche e inferenze controllate;
6. **analisi Antiskill** — Stockfish, Syzygy o modelli eseguiti secondo un profilo identificato;
7. **viste canoniche** — scelte reversibili per query, export e Rapporti.

Gli strati non si sovrascrivono. Una vista può combinarli, ma deve permettere di risalire al contributo di ciascuna fonte.

## Autorità contestuale

Non esiste una fonte globalmente "più vera" per ogni campo. L'autorità è legata al tipo di affermazione e al periodo osservato.

Esempi:

- FIDE è fonte primaria per FIDE ID, titoli e rating FIDE pubblicati;
- FSI è fonte primaria per tesseramento, società e atti federali italiani;
- un sistema torneo o un organizzatore può essere la fonte primaria per pairing e risultati di uno specifico evento;
- un albo o archivio storico curato può essere la migliore fonte disponibile per un record storico, conservando comunque la fonte citata;
- un PGN annotato è fonte delle proprie annotazioni, non automaticamente dell'identità civile dei giocatori.

La riconciliazione deve quindi registrare **campo, fonte, validità temporale, evidenza e criterio di scelta**, non soltanto un valore finale.

## Riusa prima di calcolare

Il calcolo locale è una risorsa, non il punto di partenza obbligatorio.

Per una posizione il percorso preferito è:

```text
analisi affidabile già acquisita
→ analisi esterna interrogabile/cacheabile
→ calcolo locale selettivo
→ calcolo locale profondo solo quando serve
```

Valutazioni provenienti da Lichess o da altre fonti possono evitare molto compute locale e aumentare rapidamente la copertura. Restano però **analisi di fonte**: profondità, nodi, engine, parametri e data vanno conservati quando disponibili. Se il metodo non è sufficientemente noto, il dato può essere utile per un Rapporto esplorativo ma non va spacciato per equivalente a un profilo Antiskill standardizzato.

Un esperimento scientifico può richiedere deliberatamente ricalcolo omogeneo anche quando esiste un'eval esterna. Questa è una proprietà della ricetta di Ricerca, non una ragione per scartare l'eval già disponibile dal corpus generale.

## Asserzioni di fonte persistenti

I fatti dichiarati da una fonte possono essere conservati nel contratto `antiskill_source_assertion_group_v1`. Il gruppo lega più asserzioni allo stesso `source_key` e `snapshot_id`; ogni item conserva `assertion_id`, tipo e identità del soggetto, predicato namespaced, valore JSON e validità temporale opzionale.

Questo strato serve, per esempio, a rating FIDE pubblicati, appartenenze FSI, risultati/classifiche ufficiali, albi d'oro, date e ruoli dichiarati. Non contiene un flag di verità canonica e non risolve i conflitti fra fonti: registra ciò che la fonte afferma, con provenienza.

La persistenza v1 è content-addressed in `inspector/assertions/groups/<blake3>.json.xz` e usa:

```text
antiskill inspector validate-source-assertions --input FILE.json
antiskill inspector store-source-assertions --root WORKSPACE --input FILE.json
antiskill inspector verify-source-assertions --root WORKSPACE --assertions PATH
antiskill inspector index-source-assertions --root WORKSPACE --assertions PATH [--assertions PATH ...]
```

L'indice derivato raggruppa deterministicamente per `(subject_kind, subject_id, predicate)` e conserva, per ogni asserzione concorrente, valore, validità e provenienza completa. Non elegge automaticamente una fonte o un valore canonico: rende invece espliciti e interrogabili accordi e conflitti fra snapshot/fonti.

La riconciliazione contestuale v1 è deliberatamente esplicita e fail-closed: `reconcile-source-assertion` riceve una policy con claim, data `as_of`, ordine delle fonti ammesso e motivazione. Considera soltanto asserzioni temporalmente valide, sceglie la prima fonte disponibile nell’ordine dichiarato e rifiuta valori concorrenti della stessa fonte. Date di validità malformate o intervalli impossibili fanno fallire la riconciliazione invece di far sparire silenziosamente un’evidenza. La policy è locale al claim: non crea una graduatoria globale delle fonti e l’output conserva la provenienza dell’asserzione scelta e il criterio umano/macchina dichiarato.

## Annotazioni e commenti

Annotazioni serie provenienti da libri, broadcast, autori, database, federazioni o altre fonti possono arricchire il corpus quando diritti e provenienza lo consentono.

Ogni gruppo di annotazioni dovrebbe poter identificare almeno:

```text
source_id / snapshot_id
autore o soggetto attribuito, se noto
data o intervallo, se noto
diritti / visibilità
target: partita | mossa | posizione | evento | persona
tipo: testo | NAG | variante | eval | clock | grafica | altro
metodo o engine, quando pertinente
```

Annotazioni diverse sulla stessa posizione non vengono fuse in un unico commento anonimo. La sintesi è una vista derivata.

La custodia v1 usa gruppi validati e content-addressed: `antiskill inspector store-annotations --root WORKSPACE --input FILE.json` normalizza il documento nel contratto `antiskill_annotation_group_v1`, pubblica il JSON logico compresso sotto `inspector/annotations/groups/<blake3>.json.xz` e verifica immediatamente hash, percorso e serializzazione canonica. `verify-annotations` rifà gli stessi controlli in fail-closed. Questa fase custodisce i gruppi; l'indice trasversale per target resta un incremento separato.

## Completezza come vettore

"Corpus completo" non è un booleano e non è un solo numero. La copertura va misurata almeno lungo dimensioni separate:

- **fonti** — quali giacimenti previsti sono acquisiti e quanto sono aggiornati;
- **partite** — quante partite attese sono presenti per evento/periodo/popolazione;
- **identità** — quante persone, account ed eventi sono riconciliati con sufficiente confidenza;
- **metadati** — copertura di date, round, rating, risultati, società e altri campi utili;
- **posizioni** — quota indicizzata e interrogabile per posizione/trasposizione;
- **analisi** — quota con valutazioni riusabili e qualità/profilo noto;
- **annotazioni** — presenza di commenti o analisi editoriali con provenienza; gli item custoditi nei gruppi verificati sono già conteggiabili come copertura osservata, senza implicare completezza;
- **storia e conoscenza** — albi, rating list, autorità di evento e riferimenti storici collegati.

I Rapporti devono esporre queste coperture quando incidono sull'interpretazione del risultato. Un'enorme quantità di partite non compensa automaticamente un buco di identità o di provenienza.

## Aggiornamento continuo

Ogni fonte con una cadenza nota dovrebbe poter diventare un Job ripetibile:

```text
check/sync → snapshot immutabile → parse → diff → riconciliazione → indici → lacune
```

Un controllo che non trova byte nuovi è comunque informazione di freschezza. Un cambiamento produce nuovi snapshot e derivazioni, mai una riscrittura silenziosa della storia.

Le code di lacune devono poter trasformare assenze concrete in lavoro successivo: evento senza PGN, persona senza crosswalk FIDE/FSI, posizione importante senza analisi, albo incompleto, fonte scaduta o parser rotto.

## Query prima dei prodotti

La prova che il corpus funziona non è possedere file, ma poter porre domande trasversali senza ricostruire una pipeline per ogni caso d'uso.

Esempi:

- tutte le occorrenze e le fonti di una partita logica;
- identità e cronologia verificata di un giocatore fra FIDE, FSI e account;
- partite mancanti rispetto a un evento ufficiale;
- posizioni già valutate da una fonte esterna ma non ancora da un profilo standardizzato;
- albi d'oro con evidenze e conflitti;
- commenti o analisi attribuiti a una posizione attraverso fonti differenti;
- evoluzione di repertorio con rating e contesto validi alla data della partita.

Se una verticale richiede una di queste capacità, la soluzione preferita è densificare la Base e poi riusarla.

## Acceptance del riallineamento

Consideriamo questo modello realmente implementato quando almeno quattro verticali diverse possono condividere, senza pipeline ombra:

- la stessa identità di partite/persone/eventi;
- lo stesso modello di provenienza e autorità;
- gli stessi snapshot e meccanismi di aggiornamento;
- la stessa ricerca per tag e posizione;
- lo stesso archivio di analisi esterne e calcolate;
- gli stessi contratti per annotazioni e conoscenza;
- metriche esplicite di copertura e lacune.
