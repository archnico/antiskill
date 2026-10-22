# Antiskill Inspector — contratto del motore v0

## Compressed Artifact Storage v1

I nuovi artefatti JSON immutabili in `segmentations`, `tokenizations`, `parses`,
`interpretations`, `catalogs` e `collections` sono memorizzati come XZ preset 3
con suffisso `.json.xz`. Il nome resta `<BLAKE3>.json.xz`: BLAKE3, identità e
campi `*_bytes` riguardano sempre i byte JSON canonici non compressi, incluso il
newline finale, mai il flusso XZ.

I reader accettano sia il formato storico `.json` sia `.json.xz`, decomprimendo
quest'ultimo in memoria prima della medesima validazione canonica e referenziale.
Un artefatto legacy valido viene riusato senza crearne una copia compressa; non
esiste migrazione automatica. Se sono presenti entrambe le forme viene scelta
quella compressa e ogni errore di decompressione o verifica causa un fallimento
chiuso, senza fallback alla copia legacy.

Source Manifest e SourceBlob di Raw Custody, byte PGN originali, export e file
utente restano non compressi. La compressione non è cifratura né protezione dei
dati. v1 non introduce database, migrazione, codec configurabili o streaming
della pipeline: il JSON logico viene ancora caricato e decompresso in memoria.

## Obiettivo

Importare documenti PGN senza perdere informazione, renderli interrogabili e
poter ricostruire ogni sorgente originale byte per byte.

## Invarianti

1. Nessun byte sorgente viene modificato o scartato.
2. La sorgente originale e la rappresentazione interpretata sono oggetti distinti.
3. Reimportare lo stesso contenuto non crea una seconda copia del blob.
4. Ogni dato interpretato deve essere riconducibile alla sorgente e ai relativi offset.
5. Una partita canonica non sostituisce le sue occorrenze nelle diverse fonti.
6. Una variante annotata non viene mai confusa con una mossa realmente giocata.
7. Una posizione scacchistica e la sua occorrenza contestuale sono oggetti distinti.
8. Errori e token sconosciuti vengono conservati e segnalati, mai ignorati.
9. Merge, collegamenti e deduplicazioni devono essere reversibili.
10. Dati derivati e normalizzazioni devono indicare metodo e versione.

## Active Workspace State v1

`inspector/workspace-state.json` è il solo puntatore mutabile alla Collection
attiva. Il formato JSON canonico contiene `schema_version: 1`, il profilo
`antiskill_workspace_state_v1` e `active_collection` con `path`, `blake3` e
`bytes`. Il path è relativo al workspace e deve essere il percorso canonico
content-addressed della Collection; non sono ammessi path assoluti, fallback,
scansioni o selezioni basate su nome e tempo.

Dopo un ingest completamente riuscito, `ingest-directory` verifica l'intera
catena della Collection pubblicata e soltanto allora sostituisce atomicamente
lo stato tramite un file temporaneo sincronizzato. Le Collection precedenti
restano immutabili e raggiungibili tramite il loro path. Una Collection
parziale viene conservata, ma non diventa attiva; qualsiasi ingest `PARTIAL`
lascia byte-identico uno stato già valido.

`query-collection` risolve e verifica lo stato quando `--collection` è omesso;
un override esplicito non consulta lo stato. Stato mancante, non canonico o
corrotto e Collection mancante, corrotta o con identità diversa causano un
fallimento chiuso. Lo schema v1 non prevede migrazioni: versioni o campi
sconosciuti vengono rifiutati, mentre le Collection v1 e v2 già supportate
restano compatibili.

## Workspace Collection v1

Una Collection è un manifest immutabile e content-addressed che riunisce più
SourceBlob e i Catalog verificati che li descrivono. Vive in
`inspector/collections/<blake3>.json`; il nome identifica esattamente i byte
JSON canonici del manifest. Non contiene timestamp, puntatori mutabili o
riferimenti alla Collection precedente.

`inspector ingest-directory` visita ricorsivamente una directory senza seguirne
i symlink, considera soltanto file regolari `.pgn` (senza distinzione ASCII tra
maiuscole e minuscole) e ordina i candidati per percorso relativo UTF-8. Due
osservazioni con uguali hash e dimensione condividono un solo SourceBlob e un
solo source record, ma conservano entrambi i percorsi. Questa è deduplicazione
dei byte sorgente, non deduplicazione delle partite.

Gli errori interni alla visita sono isolati per entry o sottodirectory. Un
percorso PGN non UTF-8 è rappresentato soltanto nel diagnostico runtime come
`hex:` seguito dai byte dell'`OsStr` in esadecimale minuscolo; non viene
normalizzato con caratteri sostitutivi e non entra nel manifest persistente.

L'opzione `--previous` esprime un'unione esplicita: le sorgenti e osservazioni
verificate della Collection precedente sono il punto iniziale, senza creare un
legame storico nel nuovo manifest. Per ogni nuova sorgente l'ingest riusa prima
il Catalog della Collection precedente, quindi il Catalog valido dal percorso
lessicograficamente minore già presente nel workspace. L'indice dei Catalog è
costruito una sola volta. Artefatti pubblicati prima di un arresto possono così
essere scoperti al rilancio anche senza `--previous`.

Un errore relativo a un singolo candidato non interrompe gli altri: la
Collection delle sorgenti riuscite viene pubblicata, l'esito è `PARTIAL` e il
processo termina non-zero. I percorsi osservati sono provenienza e non devono
continuare a esistere; il SourceBlob verificato resta la fonte operativa.

`query-collection` verifica prima l'intera Collection e tutti i Catalog, quindi
applica linearmente la stessa semantica AND di `query-catalog`. I risultati sono
ordinati per source id e game id e una stessa partita presente in sorgenti
diverse resta presente più volte. `export-occurrence` verifica la medesima
catena, controlla hash e intervallo della GameOccurrence e pubblica i byte
esatti atomicamente senza sovrascrivere l'output.

La v1 non risolve identità di persone o eventi e non introduce CanonicalGame,
deduplicazione di partite, indici di posizione o database. La scansione dei
Catalog, la verifica e le query multi-Catalog sono lineari; inoltre le fasi v0
della pipeline possono ancora caricare un intero sorgente in memoria.

## Exact Game Duplicates v1

`antiskill inspector duplicates --root WORKSPACE --collection COLLECTION
[--format text|jsonl]` produce un report non distruttivo delle occurrence con
byte sorgente esattamente uguali. Il profilo esplicito è
`exact_source_game_bytes_v1` e la chiave ordinata è
`(source_game_blake3, game_bytes)`, dove `game_bytes` è la sottrazione checked
`source_end - source_start`. Una differenza di un solo byte — inclusi commenti,
whitespace, line ending, NAG, RAV, ordine dei tag, spelling o annotazioni —
impedisce il match.

L'unità del report è una `GameOccurrence`, identificata soltanto da
`(source_id, game_id)`, non un `CanonicalGame`. Due game id distinti nella
stessa source possono essere duplicati. Due source logiche diverse possono
condividere uno slice identico. Più observations della stessa SourceBlob sono
mostrate come metadati nell'ordine canonico della Collection, ma non
moltiplicano le occurrence. Il comando non fonde, nasconde, riscrive o cancella
partite e non pubblica alcun artefatto.

Prima di costruire il report viene completato il loader verificato della
Collection. Esso verifica Collection → Catalog → Interpretation → Parse →
Tokenization → Segmentation → Source Manifest → SourceBlob e ricalcola il
Catalog, compreso `source_game_blake3` sullo slice esatto del blob già caricato.
Gli observed paths non vengono consultati e possono non esistere più. Qualsiasi
corruzione interrompe il comando prima della produzione di output; non esiste
un report parziale.

I gruppi sono ordinati per hash crescente e dimensione crescente; quelli con
meno di due occurrence vengono rimossi e solo dopo ricevono `group_id`
consecutivi da zero. Le occurrence interne sono ordinate per `source_id` e
`game_id`. Il formato predefinito `text` è tab-separated: contiene le tre righe
riassuntive `DUPLICATE_PROFILE`, `GROUPS`, `DUPLICATE_OCCURRENCES`, quindi righe
`GROUP` e `OCCURRENCE`, e termina con `DUPLICATES_REPORT\tPASS`. Stringhe e
array ambigui sono JSON compatti. `jsonl` emette una riga summary e una riga
compatta per gruppo, con ordine stabile dei campi e newline finale; non emette
un record PASS. Entrambi i formati sono byte-deterministici.

La raccolta è lineare nel numero di partite; il raggruppamento e gli ordinamenti
costano inoltre `O(n log n)` nel caso generale. La v1 non crea indici
persistenti né database. Deduplicazione semantica, normalizzazione PGN,
confronto di mosse o posizioni, near/fuzzy duplicates, identity resolution,
merge e cancellazione restano sviluppi futuri fuori perimetro.

## Oggetti fondamentali

### SourceBlob

Contenuto originale immutabile, identificato tramite BLAKE3 e conservato in un
object store content-addressed. Il codec fisico non partecipa all'identità del
contenuto: lo stesso blob deve restare riconoscibile anche se in futuro viene
conservato grezzo, compresso o in un contenitore differente.

### SourceDocument

Una provenienza concreta del blob: percorso osservato, nome, data e metadati
dell'importazione. Più documenti possono riferirsi allo stesso blob.

### GameOccurrence

Una partita così come appare in un preciso documento sorgente, con intervallo
di byte, ordine e rappresentazione originali.

### CanonicalGame

Identità scacchistica basata almeno su variante, posizione iniziale e sequenza
delle mosse principali. Non contiene né assorbe automaticamente annotazioni e
metadati delle fonti.

### MoveOccurrence

Una mossa collocata in una linea precisa, con ruolo esplicito: mainline,
variante sorgente, variante utente, variante motore o frammento innestato.

### PositionState

Stato scacchistico condivisibile: disposizione dei pezzi, tratto, diritti di
arrocco, en passant e variante.

### PositionOccurrence

Presenza di un PositionState in uno specifico percorso di una partita o variante.

### AnnotationGroup

Gruppo di annotazioni con autore, fonte, diritti, data e visibilità comuni.

### AnnotationItem

Commento testuale, NAG, valutazione, orologio, grafica o altro elemento
collegato a una mossa, posizione o intervallo della sorgente.

## Rappresentazioni conservate

Inspector mantiene contemporaneamente:

1. il documento sorgente originale nel formato fisico scelto dall'object store;
2. gli intervalli e i token necessari all'ispezione lossless;
3. la struttura scacchistica interpretata;
4. indici derivati e ricostruibili.

La struttura interpretata non deve duplicare inutilmente commenti e stringhe
già presenti nel blob: può riferirsi ai relativi intervalli di byte.

## Decisione sullo storage v0

1. Inspector riusa l'object store BLAKE3 già presente in antiskill.
2. Non viene introdotto un secondo archivio indipendente dei medesimi sorgenti.
3. La prima implementazione usa il formato raw già esistente come riferimento
   di compatibilità e correttezza.
4. La compressione Zstandard viene misurata come esperimento riproducibile prima
   di diventare un formato persistente.
5. Il rapporto di storage deve distinguere:
   - dimensione dei file originali osservati;
   - byte fisicamente aggiunti all'object store;
   - byte evitati dalla deduplicazione;
   - dimensione teorica o sperimentale compressa;
   - metadati e indici di Inspector.
6. Il file originale esterno al workspace non viene cancellato né modificato.
   La sua eventuale presenza sullo stesso disco viene indicata separatamente
   nel rapporto, perché non appartiene allo storage gestito da Inspector.

## Primo test verticale

Il primo test funzionante deve:

1. importare un file PGN come byte;
2. calcolarne l'hash;
3. salvarlo nell'object store content-addressed;
4. riesportarlo;
5. verificare hash e identità byte per byte;
6. reimportarlo senza duplicare il blob;
7. misurare byte originali, byte fisicamente aggiunti e metadati.

## Fuori dal primo test

- parsing completo delle mosse;
- deduplicazione scacchistica;
- ricerca per posizione;
- Stockfish;
- Player Report;
- merge di commenti;
- innesto di varianti;
- interfaccia grafica.

Queste funzioni devono restare compatibili con il modello, ma non bloccano il
primo round trip lossless.

## Secondo test verticale: segmentazione lessicale

`antiskill inspector segment --root WORKSPACE --manifest SOURCE_MANIFEST`
verifica il manifest Inspector e il relativo oggetto, quindi pubblica sotto
`inspector/segmentations/` un nuovo manifest JSON. La pubblicazione usa un file
temporaneo sincronizzato e un hard link con `create_new` equivalente: un
manifest esistente non viene mai sovrascritto e due esecuzioni producono due
manifest distinti.

Lo schema persistente v1 è:

```json
{
  "schema_version": 1,
  "source_manifest": "inspector/manifests/123.json",
  "source_manifest_blake3": "fedcba9876543210...",
  "source_manifest_bytes": 456,
  "source_blake3": "0123456789abcdef...",
  "source_bytes": 123,
  "segmented_at_unix_ms": 123,
  "segments": [
    {
      "ordinal": 0,
      "start": 0,
      "end": 20,
      "kind": "unclassified"
    },
    {
      "ordinal": 1,
      "start": 20,
      "end": 123,
      "kind": "game_candidate"
    }
  ]
}
```

`source_manifest` è un percorso relativo al workspace composto soltanto da
componenti normali e identifica il percorso di provenienza. I campi
`source_manifest_blake3` e `source_manifest_bytes` fissano invece i byte esatti
dell'occorrenza del manifest sorgente, inclusi formattazione e newline. Il
manifest non contiene copie dei byte: tutti gli intervalli si riferiscono al
`SourceBlob` identificato da `source_blake3`.

### Invarianti degli intervalli

Gli intervalli hanno semantica half-open `[start, end)`, ordinali consecutivi e
sono ordinati per `start`. Il primo parte da zero, ogni `end` coincide con lo
`start` successivo, ogni intervallo ha `start < end` e l'ultimo termina a
`source_bytes`. Ne conseguono copertura completa, assenza di buchi,
sovrapposizioni, duplicazioni e perdite. La concatenazione degli intervalli
ricostruisce byte per byte il blob.

`antiskill inspector verify-segmentation --root WORKSPACE --segmentation
SEGMENTATION` ricontrolla schema, percorsi e symlink, manifest sorgente, hash e
dimensione dell'oggetto, invarianti degli intervalli, kind supportati e hash
della ricostruzione ottenuta scandendo in ordine tutti gli intervalli. Prima di
deserializzare il manifest sorgente, la verifica ne confronta hash e dimensione
con quelli fissati dalla segmentazione: anche una modifica semanticamente valida
di un solo campo o della sola formattazione viene rifiutata.

Durante `segment`, l'oggetto viene letto una sola volta. Hash e dimensione sono
verificati sui medesimi byte passati alla scansione lessicale per generare gli
offset; in caso di incoerenza non viene pubblicata alcuna segmentazione.

### Euristica v0

La scansione opera esclusivamente su byte e separa le righe riconoscendo sia LF
sia CRLF. Una tag pair plausibile è una riga formata da:

1. spazio orizzontale opzionale;
2. `[` e un nome ASCII non vuoto composto da lettere, cifre o `_`;
3. uno spazio, una stringa tra doppi apici con escape tramite `\`, e `]`;
4. solo spazio orizzontale opzionale fino alla fine della riga.

Un `game_candidate` inizia alla prima riga di un blocco contiguo di una o più
tag pair. Per evitare di attribuire materiale ambiguo a una partita, il blocco
è candidato soltanto se, dopo eventuali righe vuote, esiste un primo blocco
contiguo di righe non vuote e non-tag; il candidato termina alla fine di quel
blocco. Tutti gli altri byte — preamboli, separatori, tag senza corpo, blocchi
successivi separati da righe vuote e materiale finale — sono `unclassified`.
Segmenti adiacenti della stessa classe possono restare distinti soltanto se
derivano da confini lessicali espliciti; la copertura resta comunque esatta.

La classificazione `game_candidate` non afferma che il contenuto sia una
partita PGN semanticamente valida. Non vengono interpretati movetext, risultati,
mosse, commenti o posizioni.

### Limiti noti

L'euristica privilegia falsi negativi espliciti. Può lasciare `unclassified`
movetext deliberatamente spezzato da righe vuote, tag pair con sintassi non
standard, partite senza tag, o tag seguiti da un corpo vuoto. Una riga
sintatticamente simile a una tag pair seguita da testo può invece produrre un
falso candidato lessicale; non viene promossa a partita valida. L'intero
documento viene attualmente letto in memoria per costruire gli offset, quindi
questa v0 non soddisfa ancora l'obiettivo generale di streaming per file molto
grandi.

## Terzo test verticale: tokenizzazione lossless

`antiskill inspector tokenize --root WORKSPACE --segmentation SEGMENTATION`
tokenizza esclusivamente i segmenti `game_candidate` e pubblica atomicamente un
manifest sotto `inspector/tokenizations/`. I segmenti `unclassified` non vengono
tokenizzati: restano custoditi integralmente dal blob e dal manifest di
segmentazione.

Lo schema persistente v1 è:

```json
{
  "schema_version": 1,
  "segmentation_manifest": "inspector/segmentations/123.json",
  "segmentation_manifest_blake3": "0123456789abcdef...",
  "segmentation_manifest_bytes": 456,
  "source_blake3": "fedcba9876543210...",
  "source_bytes": 1234,
  "tokenized_at_unix_ms": 123,
  "streams": [
    {
      "segment_ordinal": 1,
      "start": 20,
      "end": 500,
      "tokens": [
        {
          "ordinal": 0,
          "start": 20,
          "end": 21,
          "kind": "left_bracket"
        }
      ]
    }
  ]
}
```

`segmentation_manifest` è un percorso relativo al workspace direttamente
riutilizzabile con lo stesso `--root`. `segmentation_manifest_blake3` e
`segmentation_manifest_bytes` fissano i byte esatti del manifest di
segmentazione, inclusi formattazione e newline. Il file viene letto una sola
volta, identificato e deserializzato dagli stessi byte.

Ogni `game_candidate` ha esattamente uno stream, ordinato per
`segment_ordinal`; nessuno stream può riferirsi a un segmento `unclassified`.
Gli estremi dello stream coincidono con quelli del segmento. I token hanno
ordinali consecutivi e intervalli assoluti half-open `[start, end)` contigui,
non vuoti e interamente contenuti nel blob. La loro concatenazione copre
esattamente lo stream senza buchi, sovrapposizioni, duplicazioni o perdite.
I token non contengono copie o versioni decodificate dei byte.

I kind persistenti, serializzati in `snake_case`, sono:

- `whitespace`;
- `left_bracket`, `right_bracket`;
- `left_parenthesis`, `right_parenthesis`;
- `quoted_string`, `unterminated_quoted_string`;
- `brace_comment`, `unterminated_brace_comment`;
- `semicolon_comment`;
- `escape_line`;
- `move_number_candidate`;
- `result_candidate`;
- `nag_numeric`, `nag_symbolic`;
- `symbol`;
- `unknown`.

### Precedenza lessicale

La scansione è deterministica, byte-oriented e maximal-munch. A ogni offset
applica nell'ordine:

1. `escape_line`, quando `%` è in prima colonna;
2. `whitespace`;
3. `brace_comment`;
4. `semicolon_comment`;
5. `quoted_string`;
6. parentesi tonde e quadre;
7. `nag_numeric`;
8. `nag_symbolic`, provando prima le forme di due caratteri;
9. `result_candidate`;
10. `move_number_candidate`;
11. `symbol`;
12. `unknown`.

LF e CRLF sono riconosciuti senza richiedere UTF-8. Ogni passo consuma almeno
un byte. Un commento brace o una stringa senza terminatore produce il relativo
kind `unterminated_*` fino alla fine del solo segmento corrente. Un byte non
riconosciuto avvia un `unknown` che prosegue fino alla fine dello stream o al
primo offset dal quale può iniziare un token riconosciuto: anche `unknown` è
quindi una sequenza massimale. Il lexer non attraversa mai il confine del
`game_candidate`.

`antiskill inspector verify-tokenization --root WORKSPACE --tokenization
TOKENIZATION` verifica schema, percorsi e symlink, identità byte-esatta della
segmentazione, intera catena fino al manifest sorgente e al blob, identità della
sorgente, corrispondenza uno-a-uno tra stream e `game_candidate`, kind,
ordinali, intervalli e copertura completa. Dopo questi controlli strutturali,
legge il blob una sola volta, verifica hash e dimensione sui byte letti e usa
gli stessi byte per ricalcolare il lexer su ogni `game_candidate`. Gli stream
ricalcolati devono essere identici a quelli persistiti in numero, ordine,
segmento, intervalli, ordinali e kind dei token. Una modifica strutturalmente
valida ma lessicalmente falsa viene quindi rifiutata.

### Limiti della tokenizzazione v0

I kind `symbol`, `move_number_candidate` e `result_candidate` sono soltanto
classificazioni lessicali. Non vengono validati PGN, SAN, mosse, risultati,
varianti o posizioni. Il blob viene letto interamente in memoria durante la
tokenizzazione per assicurare che hash, dimensione e offset derivino dalla
stessa lettura; questa v0 non è quindi streaming per file molto grandi.

## Quarto vertical slice: parse strutturale lossless

`antiskill inspector parse --root WORKSPACE --tokenization TOKENIZATION`
costruisce una CST per ogni stream della tokenizzazione e pubblica atomicamente
un manifest sotto `inspector/parses/`. Il parser usa esclusivamente i token
verificati: non copia byte o token, non decodifica tag e non interpreta SAN,
legalità, posizioni o coerenza del risultato.

### Schema Parse v1

```json
{
  "schema_version": 1,
  "tokenization_manifest": "inspector/tokenizations/123.json",
  "tokenization_manifest_blake3": "...",
  "tokenization_manifest_bytes": 123,
  "source_blake3": "...",
  "source_bytes": 40990,
  "parsed_at_unix_ms": 123,
  "trees": [
    {
      "segment_ordinal": 1,
      "token_count": 52,
      "nodes": [
        {
          "id": 0,
          "parent_id": null,
          "child_ordinal": null,
          "kind": "game_candidate",
          "status": "valid",
          "start_token": 0,
          "end_token": 52
        }
      ],
      "diagnostics": [
        {
          "ordinal": 0,
          "code": "unexpected_token_range",
          "node_id": 1,
          "start_token": 10,
          "end_token": 12
        }
      ]
    }
  ]
}
```

`tokenization_manifest` è relativo al workspace; hash e dimensione fissano i
byte esatti del manifest di tokenizzazione. Gli intervalli dei nodi e delle
diagnostiche sono intervalli half-open di ordinali locali allo stream, non
offset byte. I nodi formano un'arena piatta serializzata in preordine: ID
consecutivi, genitore sempre precedente, `child_ordinal` consecutivi e radice
unica `game_candidate`. I figli diretti di ogni contenitore partizionano senza
buchi né sovrapposizioni l'intervallo del genitore; le foglie coprono quindi
ogni token esattamente una volta.

I kind, serializzati in `snake_case`, sono `game_candidate`, `tag_section`,
`tag_pair`, `movetext`, `variation`, `delimiter`, `trivia`, `escape_line`,
`tag_name_candidate`, `tag_value_candidate`, `move_number`,
`move_symbol_candidate`, `result`, `nag`, `comment` e `unparsed`. Gli stati
sono `valid`, `recovered`, `incomplete` e `unparsed`. Solo una foglia
`unparsed` usa lo stato omonimo; nei contenitori `incomplete` prevale su
`recovered`, che prevale su `valid`.

Le diagnostiche non contengono testo. I codici persistenti sono
`malformed_tag_pair`, `unterminated_tag_pair`, `unterminated_tag_value`,
`unterminated_comment`, `unterminated_variation`, `empty_variation`,
`stray_right_parenthesis`, `tag_pair_outside_tag_section` e
`unexpected_token_range`. Ordinali e ordine di creazione sono deterministici e
ogni intervallo diagnostico è contenuto nel nodo più specifico pertinente.

### Grammatica prudente e recovery

All'inizio dello stream una tag pair completa richiede `left_bracket`, trivia
eventuale, `symbol`, almeno un `whitespace`, `quoted_string`, trivia eventuale
e `right_bracket`. Sono riconosciute come incomplete soltanto un valore
`unterminated_quoted_string`, oppure un valore quoted valido seguito da trivia
e fine stream senza parentesi quadra destra. Dopo una tag pair, il whitespace
resta nella `tag_section` solo se il look-ahead trova un'altra tag pair completa
o prudentemente incompleta; altrimenti apre il `movetext`. Una forma bracket-led
confusa resta `unparsed`. Una tag pair riconoscibile dopo l'inizio del movetext
è conservata come un solo `unparsed` con `tag_pair_outside_tag_section`.
Ogni tentativo bracket-led riceve il limite esclusivo del contesto sintattico
corrente. Nel movetext di una variazione non può cercare una parentesi quadra
destra oltre la parentesi tonda che chiuderebbe quella variazione: tag complete,
incomplete e recovery malformate restano sempre entro tale confine.

Nel movetext whitespace, escape line, commenti, numeri di mossa candidati,
symbol, risultati e NAG diventano foglie strutturali senza interpretazione. Un
risultato non termina il movetext. Materiale inatteso consecutivo è aggregato
in un `unparsed` massimale. Commenti brace non terminati sono `incomplete`.

Una parentesi sinistra apre una `variation` con delimiter iniziale, movetext
interno non vuoto e delimiter finale quando presente. Le variazioni sono
ricorsive. La parentesi finale mancante rende la variazione `incomplete`; una
parentesi destra isolata resta `unparsed`. Una variazione senza contenuto
sostanziale, anche se contiene solo trivia, è `recovered` con
`empty_variation`. Ogni recovery conserva e copre comunque tutti i token.
La recovery di una tag o di altro materiale inatteso non attraversa mai la
chiusura della variazione contenitrice.

La profondità massima persistibile delle variazioni è 256. Quando una
`left_parenthesis` supererebbe il limite, il parser non apre un altro livello:
conserva una sequenza consecutiva di parentesi sinistre come una singola foglia
`unparsed` lossless e aggiunge `unexpected_token_range`. La scansione prosegue
deterministicamente e le parentesi successive restano coperte una sola volta.
Anche la profondità complessiva dell'arena è limitata coerentemente a 515
livelli, includendo radice, contenitori movetext e foglie. I manifest persistiti
vengono controllati in tempo lineare tramite `parent_id` e sono rifiutati prima
di qualsiasi visita dipendente dalla profondità se superano questi limiti.
Flattening e verifica del preordine usano stack espliciti.

`antiskill inspector verify-parse` verifica schema, sicurezza dei percorsi,
identità byte-esatta e intera catena parse -> tokenizzazione -> segmentazione
-> manifest sorgente -> blob. Il loader ricalcola prima il lexer sugli stessi
byte del blob usati per hash e dimensione. Dopo i controlli strutturali
dell'arena, il verificatore riesegue il parser deterministico sui token
verificati e richiede uguaglianza esatta di alberi, nodi, stati, gerarchia,
intervalli e diagnostiche. Anche una modifica formalmente valida ma diversa
dal risultato del parser viene rifiutata.

### Limiti della parse v0

Questa fase riconosce soltanto struttura concreta e recovery conservativa.
Non valida SAN, mosse, scacchiera, posizioni, risultato, linea principale o
identità della partita. Manifest, tokenizzazione e strutture di parse vengono
caricati in memoria; la v0 non è adatta allo streaming di singoli stream molto
grandi. I limiti di profondità sono protezioni operative contro input
patologici, non interpretazioni della correttezza scacchistica di una variante.

## Interpretation Manifest v1

`antiskill inspector interpret --root WORKSPACE --parse PARSE` carica e verifica
una Parse v1 e l'intera catena fino al SourceBlob, interpreta mainline e RAV in
memoria, quindi converte esplicitamente il risultato nel profilo persistente
`antiskill_standard_lines_v1`. Il formato non serializza tipi runtime,
`ParsedPosition`, tipi o hash interni Shakmaty. Non contiene intenzionalmente un
timestamp: a parità di Parse, percorso normalizzato e profilo, i byte devono
essere identici.

Il manifest conserva, nell'ordine, versione e profilo, percorso relativo della
Parse, BLAKE3 e dimensione byte-esatti della Parse, identità del SourceBlob e i
record delle partite. Ogni partita contiene stato e variante, posizione
iniziale, tag rilevanti, mosse, linee e diagnostiche. Le posizioni sono sei
componenti FEN esplicite; le mosse normalizzate conservano UCI, case, promozione,
kind e SAN canonica. Tutti gli `Option` sono presenti nel JSON e diventano
`null` quando assenti; gli enum sono `snake_case`.

L'ordine è deterministico: partite in ordine Parse; linee e mosse secondo gli
ID depth-first prodotti dall'interprete; tag in ordine sorgente; diagnostiche
in ordine di emissione; riferimenti di mosse e risultati nel loro ordine. Il
JSON pretty terminato da newline viene identificato dal BLAKE3 dei suoi byte
completi e pubblicato come
`inspector/interpretations/<manifest_blake3>.json`. Non viene sovrascritto: un
target esistente è riusato solo se byte-identico.

`antiskill inspector verify-interpretation --root WORKSPACE --interpretation
INTERPRETATION` verifica percorso, schema e struttura, fissa l'identità
byte-esatta della Parse, ricontrolla Parse → Tokenization → Segmentation →
SourceBlob, ricalcola l'interpretazione dagli oggetti già verificati e richiede
uguaglianza esatta dell'intero manifest. Una modifica anche formalmente valida
di SAN, posizione o gerarchia RAV viene quindi rifiutata.

La v1 applica soltanto le regole Standard. Chess960 è riconosciuta ma non
interpretata. Commenti e NAG restano conservati nella CST senza annotazione
semantica. Non introduce database, ricerca, compressione, deduplicazione
semantica o export PGN canonico.

## Catalog Manifest v1

`antiskill inspector catalog` deriva una vista persistente da una Interpretation
Manifest v1 già verificata byte per byte e, attraverso essa, dalla Parse v1,
dalla Tokenization v1 e dal SourceBlob verificati. Il profilo è
`antiskill_game_catalog_v1`; non contiene timestamp ed è pubblicato come
`inspector/catalogs/<BLAKE3>.json`, dove il nome è il BLAKE3 dei byte JSON
pretty canonici completi, inclusa una singola newline finale.

Ogni record partita conserva identità e stato scacchistico essenziale deciso
dall'Interpretation, l'intervallo assoluto `[start,end)` del TokenStream nel
SourceBlob e `source_game_blake3`, calcolato direttamente sui byte sorgente di
quell'intervallo senza trimming o ricostruzione. Non duplica linee, mosse o
posizioni. Ogni `TagPair` figlio diretto di ogni `TagSection` diretto della
radice viene catalogato in ordine sorgente: Seven Tag Roster, tag tecnici,
duplicati e forme malformate restano distinti con nodi CST, intervalli token e
byte e stato di decodifica. I TagPair recuperati fuori dall'header non vengono
promossi.

La decodifica distingue un valore assente o incompleto (`missing_value`) da un
escape presente ma non ammesso (`invalid_escape`): una coppia di virgolette
esterne completa è obbligatoria e sono accettati soltanto `\"` e `\\`. I range
singolari di nome e valore sono presenti esclusivamente quando il rispettivo
candidato CST è unico; con zero o più candidati restano `null`, mentre tutti gli
ID candidati continuano a essere conservati in ordine.

`verify-catalog` ricarica e verifica l'intera catena Catalog → Interpretation →
Parse → Tokenization → SourceBlob, ricalcola record, tag e hash dei segmenti,
rinormalizza i percorsi e confronta struttura, serializzazione, byte, hash e
dimensione. Differenze di whitespace, ordine dei campi, newline, alias di
percorso o campi sconosciuti sono rifiutate.

`query-catalog` filtra deterministicamente per giocatore (`White` o `Black`) ed
evento con substring Unicode case-insensitive; i nomi dei tag sono confrontati
ASCII case-insensitive. `Date` riconosce soltanto valori completi `YYYY.MM.DD` e
gli argomenti `--date`, `--date-from` e `--date-to` usano `YYYY-MM-DD`, calendario
gregoriano ed estremi inclusivi. Filtri diversi sono in AND e tutti i duplicati
decoded partecipano. Senza filtri restituisce tutte le partite in ordine ID.
Ogni riga `GAME` espone per White, Black, Event, Date e Result array JSON
compatti contenenti tutti i valori decoded in ordine sorgente, duplicati
compresi. Result è informativo: Catalog v1 non offre né filtro result né limit.

`export-game` verifica l'intera catena e scrive atomicamente, senza overwrite,
lo slice SourceBlob `[source_start,source_end)` dopo averne ricalcolato
`source_game_blake3`. Conserva quindi byte per byte CRLF, commenti, NAG, RAV e
ogni spelling originale. Il record descrive una `GameOccurrence`, non un
`CanonicalGame`.

Un Catalog descrive una sola Interpretation. Non introduce identità di
giocatore, database globale, deduplicazione, fuzzy matching o ricerca per
posizione.

## Game occurrence v1

`antiskill inspector create-occurrences --root WORKSPACE --catalog CATALOG`
accetta esclusivamente un Catalog completamente verificato e materializza
`antiskill_game_occurrence_v1` in
`inspector/occurrences/<BLAKE3>.json.xz`. Il BLAKE3 del nome identifica i byte
JSON pretty canonici non compressi, inclusa una newline finale; lo storage
comune accetta anche la forma legacy `.json`, senza migrazione. Se esistono
entrambe le forme, quella compressa è autorevole e una sua corruzione causa un
fallimento senza fallback.

Il manifest non contiene timestamp. Conserva percorso relativo, BLAKE3 e
dimensione logica di Catalog, Interpretation, Parse, Tokenization,
Segmentation e Source Manifest, oltre all'identità del SourceBlob. I path sono
necessari soltanto al loader nel workspace. Ogni record portabile conserva
invece nella `provenance` soltanto BLAKE3 e dimensioni logiche delle sei fasi.

`occurrence_id` è il BLAKE3 della preimmagine ASCII byte-esatta seguente, dove
`\0` è un byte NUL, i numeri sono decimali senza segno e `\n` è una newline:

```text
antiskill_game_occurrence_v1\0<source_blake3>\0<source_bytes>\0<source_start>\0<source_end>\0<source_game_blake3>\n
```

I record sono ordinati per `ordinal` e mantengono l'allineamento uno-a-uno fra
segmento `game_candidate`, TokenStream, ParseTree, gioco Interpretation e gioco
Catalog. `syntax_status` copia direttamente lo stato della radice
`GameCandidate` e ammette esclusivamente `valid`, `recovered` e `incomplete`;
`interpretation_status`, `variant` e `tags` sono copiati dal Catalog senza
riselezione. Le diagnostiche sintattiche mantengono codice, nodo e range token,
aggiungendo range byte assoluti ricavati dal TokenStream. Le diagnostiche
interpretative sono copiate senza classificazioni di severità aggiuntive.

Il loader di `verify-occurrences` verifica percorso, suffisso, file regolare,
schema, struttura, identità logiche, intervalli, unicità e provenienza; ricarica
poi Catalog e l'intera catena fino al SourceBlob, ricostruisce il manifest e ne
confronta struttura, serializzazione canonica, byte, BLAKE3 e dimensione.
Ricalcola quindi implicitamente anche ogni slice sorgente, ogni
`source_game_blake3` e ogni `occurrence_id`. Campi sconosciuti, alias, traversal,
symlink esterni e qualunque mutazione verificabile sono rifiutati.

`export-occurrences` emette un oggetto JSON compatto per occurrence, nello
stesso ordine degli ordinal, con una newline per riga. Se `--output` è omesso i
byte vanno a stdout; l'intero artefatto è verificato e il JSONL è costruito
prima di qualsiasi scrittura. `export-occurrence` cerca l'identità stabile,
verifica tutta la catena e pubblica atomicamente, senza overwrite, soltanto
`SourceBlob[source_start..source_end]` dopo avere ricontrollato dimensione e
hash. Non rigenera né normalizza PGN.

`unparsed` non è uno stato persistibile in `syntax_status`: resta lo stato dei
nodi interni `NodeKind::Unparsed` nella Parse verificata. La loro presenza è
conservata losslessly dalla Parse e dalle corrispondenti diagnostiche
sintattiche dell'occurrence; sulla radice viene propagata come `recovered`,
salvo la prevalenza di `incomplete` prevista dalla Parse v1. Game occurrence v1
non altera questa semantica upstream e rifiuta una radice artificialmente
`unparsed`.

## Canonical mainline v1

`create-mainlines --root WORKSPACE --occurrences OCCURRENCES` accetta soltanto
un artefatto `game_occurrence_v1` verificato e usa l'Interpretation verificata
da esso già identificata. La proiezione non legge né riparsa SAN o FEN e non
invoca regole scacchistiche: copia esclusivamente i valori persistiti. La linea
principale è sempre `game.lines[0]`, con id zero, nessun parent, variation o
replaces e depth zero. Tutte le mosse elencate da `move_ids`, incluse quelle
non applied, e tutti i nodi Result sono conservati in ordine sorgente. Per ogni
Result il loader ricontrolla sul SourceBlob il valore esatto `1-0`, `0-1`,
`1/2-1/2` oppure `*` e i relativi intervalli token e byte assoluti.

Un record è confrontabile soltanto se la variante è Standard e supported, la
posizione iniziale persistita è disponibile (anche recovered), la mainline è
complete e non vuota, e ogni sua mossa è applied con tutti i campi normalizzati
richiesti. Lo status globale del gioco non è una condizione: una RAV rotta non
avvelena una mainline completa. Le ragioni, nell'ordine congelato, sono
`non_standard_variant`, `unsupported_variant`, `missing_initial_position`,
`invalid_initial_position`, `ambiguous_initial_position`,
`mainline_not_complete`, `empty_mainline`, `non_applied_move`.

Ogni mossa conserva `ordinal_in_mainline`, `interpretation_move_id`, status,
range token e byte, SAN sorgente, posizioni precedente e risultante,
normalizzazione e indicatore di SAN canonica. Ogni result conserva il proprio
`ordinal`, node id, range token e byte assoluti e valore byte-esatto.

La FEN iniziale concatena i sei componenti già persistiti (piece placement,
side to move, castling, en passant, halfmove clock e fullmove number), usando
`-` per castling vuoto ed en passant assente. Per i soli record confrontabili,
`canonical_mainline_id` è il BLAKE3 lowercase della preimmagine byte-esatta:

```text
antiskill_canonical_mainline_v1\0standard\0<initial_fen>\0<move_count>\0<uci_1>\0...<uci_n>\0\n
```

I Result non partecipano all'identità. Il manifest, senza timestamp, è JSON
pretty canonico con newline finale ed è pubblicato come
`inspector/mainlines/<BLAKE3>.json.xz`; la forma legacy `.json` resta leggibile
e una copia compressa presente è autorevole e fail-closed. `verify-mainlines`
ricarica occurrences e l'intera catena fino al SourceBlob, riallinea ogni
record e ricalcola manifest, serializzazione, hash e dimensione byte per byte.
`export-mainlines` produce JSONL compatto in ordine di ordinal, interamente
verificato e costruito prima della scrittura; senza `--output` usa stdout e con
un file applica pubblicazione atomica senza overwrite, traversal o symlink.
