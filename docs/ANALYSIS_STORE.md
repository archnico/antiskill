# Store delle analisi

Stato: contratto architetturale della Base con un primo artefatto persistente v1 eseguibile. Il modello logico resta indipendente dal futuro indice/database globale; il formato v1 serve a custodire e verificare risultati riusabili senza anticipare il backend di ricerca. Si applica nel quadro del [corpus verificato e arricchito](CORPUS_VERIFICATO.md).

## Obiettivo

Ogni valutazione utile deve poter essere calcolata una volta, conservata con la propria provenienza e riutilizzata in tutte le partite che raggiungono lo stesso stato scacchistico compatibile.

Il sistema deve evitare tre errori:

1. ricalcolare una posizione quando esiste già informazione sufficiente al requisito del chiamante;
2. riutilizzare come equivalenti analisi che dipendono da condizioni diverse;
3. trasformare un'analisi importata da una fonte in una falsa run Antiskill omogenea.

Il principio operativo è **reuse-first**: prima si cerca informazione già disponibile e qualificata; il compute locale colma una lacuna o produce deliberatamente un profilo standardizzato.

La valutazione non viene copiata dentro ogni partita. Le partite contengono occorrenze che puntano a una posizione condivisa; la posizione punta alle analisi disponibili.

```text
partita -> occorrenza -> posizione -> analisi
```

## Tre livelli distinti

### 1. Posizione canonica

`position_id` identifica lo stato che determina le mosse legali ordinarie:

- variante scacchistica;
- disposizione dei pezzi;
- colore al tratto;
- diritti di arrocco;
- casa en passant soltanto quando la cattura en passant è legalmente possibile.

Non partecipano a `position_id`:

- numero della mossa;
- contatore delle cinquanta mosse;
- storia delle ripetizioni;
- giocatori, Elo, tempo, evento e risultato;
- commenti e annotazioni.

La rappresentazione canonica deve essere indipendente dal testo FEN originale. FEN diversi ma semanticamente equivalenti devono produrre lo stesso `position_id`.

Il contratto v1 eseguibile è definito in [Identità posizione](POSITION_IDENTITY.md).

### 2. Stato regolamentare della partita

`rule_state_id` estende `position_id` con ciò che può cambiare il valore concreto della posizione nella partita:

- contatore delle cinquanta mosse;
- stato delle ripetizioni e possibilità di reclamo;
- eventuali informazioni storiche richieste dalla variante.

La storia completa resta nell'occorrenza della partita. `rule_state_id` è una chiave derivata e non sostituisce la linea originale.

### 3. Occorrenza

`position_occurrence` collega una posizione a un punto preciso di una partita e conserva almeno:

- `game_id`;
- indice della semimossa;
- `position_id`;
- `rule_state_id` quando disponibile;
- mossa precedente e mossa giocata;
- tempo residuo e tempo impiegato, se presenti;
- provenienza del dato;
- riferimenti ad annotazioni, commenti e valutazioni originali.

Più occorrenze possono puntare allo stesso `position_id` senza duplicare la posizione o le analisi.

## Due ambiti di analisi

### Analisi posizionale standardizzata

Una `board_analysis` valuta `position_id` in un contesto deliberatamente neutro e riproducibile, per esempio senza storia di ripetizione e con un contatore convenzionale delle cinquanta mosse.

È l'analisi predefinita per le statistiche di antiskill e può essere collegata a tutte le occorrenze della posizione. Rappresenta la qualità scacchistica della decisione, non garantisce sempre il risultato regolamentare esatto della singola partita.

### Analisi dello stato concreto

Una `state_analysis` valuta `rule_state_id` e conserva il contesto regolamentare effettivo.

È necessaria quando contano:

- regola delle cinquanta mosse;
- ripetizioni;
- finali risolti dalle tablebase;
- ricerche che devono riprodurre esattamente la decisione nella partita.

Una `state_analysis` non deve essere promossa automaticamente ad analisi universale di tutte le occorrenze dello stesso `position_id`.

## Origine e metodo dell'analisi

Ambito scacchistico, origine e metodo sono dimensioni indipendenti. Un artefatto deve dichiarare almeno:

- `origin`: `external_source`, `local_run` o `contributed_run`;
- `method`: engine, tablebase, modello di scelta umana o altro metodo versionato;
- soggetto: `position_id` o `rule_state_id`;
- provenienza e diritti;
- qualità osservabile del risultato.

Una valutazione importata da una fonte esterna conserva `source_key`, identità dello snapshot o della risposta acquisita e locator del record. Se la fonte dichiara motore, profondità, nodi, MultiPV o altri metadati, vengono conservati come asserzioni della fonte. I campi mancanti restano mancanti: Antiskill non inventa un `engine_build_id` o un `search_profile_id` completo.

Una run controllata da Antiskill può invece dichiarare pienamente build, opzioni e limiti di calcolo. Le due famiglie possono convivere sulla stessa posizione e servire gli stessi Rapporti quando il requisito lo consente, ma non vengono fuse in un unico record.

Esempio: una valutazione già disponibile in un database pubblico può essere sufficiente per selezionare rapidamente posizioni interessanti; uno studio confermativo che richiede Stockfish con build, opzioni e limite nodi congelati deve invece usare o produrre un'analisi che soddisfi esattamente quel profilo.

## Artefatto persistente v1

La prima superficie eseguibile è `antiskill_analysis_artifact_v1`. Un artefatto conserva almeno:

- soggetto e scope (`position` oppure `rule_state`);
- origine e `origin_identity`;
- per `external_source`, `source_key` e `snapshot_id` obbligatori;
- diritti e metodo;
- identità di engine/profile quando realmente note;
- stato `completed|partial`;
- righe MultiPV distinte con punteggio grezzo, prospettiva, normalizzazioni disponibili, PV e metriche osservabili.

La persistenza è content-addressed in `inspector/analyses/artifacts/<blake3>.json.xz` e viene riletta e verificata dopo la pubblicazione. Reimportare byte logici identici non crea un secondo artefatto. Le superfici di conformità sono:

```text
antiskill inspector validate-analysis --input FILE.json
antiskill inspector store-analysis --root WORKSPACE --input FILE.json
antiskill inspector verify-analysis --root WORKSPACE --analysis PATH
```

Il primo adapter di fonte è Lichess evaluations. `store-lichess-eval-record` accetta un singolo record JSON del dump ufficiale, calcola il `position_id` canonico dalla FEN, conserva separatamente ogni valutazione del record e tutte le relative PV, converte `knodes` in nodi senza perdere profondità e registra `source_key`, `snapshot_id`, locator e diritti CC0. I punteggi importati mantengono la prospettiva del Bianco usata dal modello `WhiteScore` dell'eval cache Lichess, incluso `mate: 0` per una posizione già matta. Poiché il dump non espone una build Stockfish o un profilo uniforme per record, `engine_build_id` e `search_profile_id` restano intenzionalmente assenti.

```text
antiskill inspector store-lichess-eval-record --root WORKSPACE --input RECORD.json --snapshot-id ID [--observed-at DATE]
```

Questa superficie è deliberatamente **single-record**: serve ingest selettivo e acceptance dell'adapter. Il dump bulk deve usare un formato a pack/chunk e un indice posizione dedicato; non va materializzato come centinaia di milioni di file artefatto indipendenti.

Lo store v1 non è ancora un indice globale `position_id -> analyses` e non avvia compute. Esiste però un primo selettore **reuse-first** sui candidati esplicitamente forniti: il chiamante dichiara un `antiskill_analysis_requirement_v1` e Antiskill verifica ogni artefatto prima di decidere se è riusabile.

## Identità del motore

`engine_build_id` identifica in modo immutabile il programma che ha prodotto il risultato. Deve conservare, quando disponibili:

- nome del motore;
- versione dichiarata;
- hash del binario;
- architettura o build rilevante;
- hash della rete neurale;
- licenza e provenienza del binario.

Il nome `Stockfish 18` da solo non basta: build e reti differenti restano distinguibili.

## Profilo di ricerca

`search_profile_id` identifica tutte le condizioni che possono rendere due ricerche non equivalenti:

- opzioni UCI effettive;
- thread e hash;
- MultiPV;
- limite per nodi, profondità, tempo o altra condizione di arresto;
- eventuale insieme di mosse radice (`searchmoves`);
- configurazione Syzygy;
- regola delle cinquanta mosse;
- schema e versione del normalizzatore.

Le opzioni vengono serializzate in ordine canonico prima di calcolare l'identificatore.

## Esecuzioni e risultati

Ogni `analysis_run` è immutabile e contiene:

- soggetto: `position_id` o `rule_state_id`;
- `engine_build_id`;
- `search_profile_id`;
- istante e macchina di origine;
- applicazione o processo chiamante;
- stato: completata, interrotta, fallita o parziale;
- nodi, tempo, profondità e profondità selettiva finali;
- contatore di accessi alle tablebase, se dichiarato;
- collegamento al log di cattura.

Ogni variante principale è un record separato ordinato per `multipv_rank` e conserva:

- punteggio grezzo dichiarato dal motore;
- prospettiva del punteggio;
- punteggio normalizzato dal punto di vista del Bianco e del lato al tratto;
- matto, se dichiarato;
- WDL, se dichiarato;
- sequenza di mosse;
- profondità, nodi e tempo associati alla riga finale.

Il dato grezzo e quello normalizzato restano entrambi disponibili.

## Requisito del chiamante

Il riuso non dipende da una graduatoria universale. Ogni Rapporto, esperimento o integrazione esprime un `analysis_requirement` versionato che può richiedere, per esempio:

- `position_id` oppure `rule_state_id`;
- metodo ammesso;
- analisi standardizzata oppure qualunque fonte sufficientemente qualificata;
- MultiPV minimo;
- soglia minima di nodi/profondità quando significativa;
- necessità di WDL, mate score o linea principale;
- compatibilità con la regola delle cinquanta mosse;
- classi di provenance o diritti ammesse per l'output.

La selezione della migliore analisi è quindi sempre relativa a un requisito esplicito. Se nessun artefatto lo soddisfa, la lacuna può generare un Job di calcolo; non autorizza ad abbassare silenziosamente i requisiti.

## Requisito eseguibile e selezione reuse-first v1

`antiskill_analysis_requirement_v1` rende operativo il principio secondo cui la migliore analisi è relativa al prodotto richiesto, non a una graduatoria universale. Il requisito dichiara:

- `subject_scope` e `subject_id` esatti;
- origini ammesse e un ordine esplicito `origin_preference`;
- metodi ammessi e un ordine esplicito `method_preference`;
- `selection_priority`, che sceglie e ordina esplicitamente gli assi rilevanti fra `origin`, `method`, `completion`, `multipv`, `nodes`, `depth`;
- classi di diritti ammesse;
- eventuali `engine_build_id` e `search_profile_id` obbligatori;
- ammissibilità o meno di risultati parziali;
- MultiPV minimo e soglie opzionali di profondità/nodi;
- necessità di WDL e/o PV.

Gli ordini di preferenza devono contenere **esattamente** gli insiemi ammessi. `selection_priority` deve essere non vuoto e senza duplicati, ma può usare solo gli assi che la ricetta considera realmente significativi: un asse omesso non influenza la scelta. Non esiste quindi una precedenza implicita nascosta nel codice fra provenienza, metodo e qualità osservabile. `completion` preferisce risultati completati, mentre `multipv`, `nodes` e `depth` preferiscono maggiore copertura/lavoro osservato quando la ricetta include quegli assi. Metadati di profondità o nodi mancanti non ricevono credito nel confronto. Se due candidati restano equivalenti secondo tutti gli assi dichiarati, il BLAKE3 dell'artefatto è usato soltanto come spareggio deterministico, non come giudizio di qualità.

Le superfici iniziali sono:

```text
antiskill inspector validate-analysis-requirement --input REQUIREMENT.json
antiskill inspector select-analysis --root WORKSPACE --requirement REQUIREMENT.json --analysis PATH [--analysis PATH ...]
```

`select-analysis` opera deliberatamente solo su percorsi candidato espliciti e li verifica tramite lo store content-addressed prima della selezione. Se nessun candidato soddisfa tutte le condizioni, termina con errore: non abbassa silenziosamente MultiPV, soglie, metodo, profilo o diritti. L'indice globale `position_id -> analyses`, la discovery automatica dei candidati e l'eventuale generazione di un Job di compute restano strati successivi.

## Deduplicazione

La deduplicazione avviene soltanto per identità esatta dell'analisi. `origin_identity` identifica la run controllata oppure la provenienza esterna concreta; due fonti che riportano lo stesso punteggio non diventano per questo la stessa evidenza:

```text
analysis_identity = hash(
    subject_scope,
    subject_id,
    origin_identity,
    method_identity,
    profile_identity_if_known,
    terminal_output
)
```

Reimportare lo stesso risultato non crea duplicati. Un'analisi più profonda non cancella e non modifica quella precedente.

Il concetto di "migliore analisi disponibile" appartiene a una vista derivata, non all'identità. La vista può preferire, entro gruppi confrontabili:

1. risultati esatti di tablebase;
2. ricerche completate rispetto a quelle parziali;
3. maggiore copertura MultiPV;
4. maggiore lavoro di ricerca misurabile;
5. profili dichiarati preferiti dal progetto.

Motori, versioni, reti o profili non confrontabili restano fonti parallele. La sola profondità non autorizza una sostituzione.

## Syzygy

Le tablebase sono una fonte distinta, non una normale valutazione in centipawn.

Un risultato Syzygy deve conservare:

- insieme e versione dei file usati;
- hash o manifest dell'installazione;
- metrica disponibile, per esempio WDL o DTZ;
- contatore delle cinquanta mosse usato nella richiesta;
- eventuali limitazioni della sonda;
- risultato e mosse che lo preservano.

La sonda deve essere eseguita soltanto su stati compatibili con la tablebase e con la variante. Le posizioni con diritti di arrocco non devono essere trattate come automaticamente risolte da Syzygy.

Quando il risultato dipende dal contatore delle cinquanta mosse, viene collegato a `rule_state_id`; non viene attribuito indistintamente a tutte le occorrenze di `position_id`.

## Cattura trasparente delle sessioni UCI

Un futuro `antiskill engine proxy` si comporterà come un motore UCI verso SCID, Cute Chess e altri programmi:

```text
applicazione -> antiskill engine proxy -> motore
```

Il proxy inoltra i comandi e produce un journal append-only. L'importazione nel database avviene fuori dal percorso critico, così una scrittura lenta non rallenta il motore.

Modalità previste:

- `off`: nessuna cattura;
- `collect`: conserva posizione, configurazione e risultato finale utile;
- `research`: conserva anche progressione, checkpoint e dialogo UCI completo.

I journal vengono chiusi atomicamente, compressi e importati in modo idempotente. Un journal non viene eliminato prima che hash, importazione e verifiche risultino completati. La politica di rotazione distingue i transcript completi opzionali dai record normalizzati permanenti.

## Procedura di riuso

Prima di avviare nuovo compute:

1. canonicalizzare la posizione e determinare lo scope regolamentare;
2. costruire l'`analysis_requirement` del chiamante;
3. cercare risultati già presenti, indipendentemente dal fatto che siano importati o prodotti localmente;
4. preferire risultati esatti quando applicabili e compatibili;
5. selezionare soltanto artefatti che soddisfano il requisito dichiarato;
6. consultare cache o fonti esterne consentite dalla ricetta quando manca informazione locale;
7. calcolare soltanto l'informazione ancora mancante;
8. registrare il nuovo risultato senza sovrascrivere i precedenti.

Un MultiPV 1 non soddisfa una richiesta MultiPV 5; una ricerca limitata a `searchmoves` non equivale a una ricerca completa della posizione; un'eval esterna senza profilo completo non soddisfa una ricetta `standardized` anche quando il suo punteggio sembra plausibile.

## Provenienza e diritti

Ogni analisi conserva:

- chi o cosa l'ha prodotta o pubblicata;
- sorgente della posizione;
- sorgente dell'analisi, quando distinta;
- snapshot/risposta e locator del record per le analisi importate;
- metodo di acquisizione;
- licenza o classe dei diritti;
- eventuali restrizioni alla redistribuzione.

Analisi locali, contributi volontari e database esterni non vengono fusi in un corpus redistribuibile indistinto. La vista può combinarli, ma l'esportazione deve rispettare la provenienza di ogni record.

Un futuro sistema cooperativo potrà accettare pacchetti firmati di analisi. La convalida dovrà verificare legalità della posizione, identità del motore, riproducibilità minima, coerenza delle varianti e assenza di duplicati prima della promozione a fonte fidata.

## Query che il modello deve rendere possibili

- recuperare tutte le partite che hanno raggiunto una posizione;
- ottenere la migliore analisi disponibile secondo un requisito dichiarato;
- sapere se il requisito può essere soddisfatto senza nuovo compute;
- distinguere analisi importate, locali e contribuite sulla stessa posizione;
- confrontare motori o versioni sulla stessa posizione;
- trovare quali mosse legali non sono ancora state valutate;
- misurare la scelta umana per fascia Elo, cadenza e tempo residuo;
- distinguere valutazione posizionale standardizzata e valore regolamentare concreto;
- ricostruire esattamente la provenienza di ogni punteggio.

## Invarianti aggiuntive

1. Nessuna analisi utile viene sovrascritta da una successiva.
2. Nessuna valutazione viene duplicata per ogni partita che raggiunge la posizione.
3. Ogni punteggio conserva la prospettiva originale e quella normalizzata.
4. Ogni scelta della "migliore analisi" è una vista riproducibile e versionata.
5. Posizione canonica, stato regolamentare e occorrenza restano concetti distinti.
6. Un risultato contestuale non viene riutilizzato come universale senza una regola esplicita.
7. Il log di cattura e il record normalizzato restano collegati ma separati.
8. Importazione, deduplicazione e riuso devono essere deterministici e idempotenti.
9. Un dato importato non viene promosso retroattivamente a run Antiskill standardizzata.
10. Un nuovo compute parte solo dopo una ricerca esplicita di artefatti già adeguati al requisito.
