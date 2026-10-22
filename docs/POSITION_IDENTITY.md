# Identità posizione v1

## Scopo

`position_id` è la chiave stabile e ricostruibile con cui la Base Antiskill riconosce la stessa posizione scacchistica attraverso partite, trasposizioni, fonti e analisi differenti.

Questo contratto riguarda **Scacchi standard**. Non definisce ancora `rule_state_id`, l'indice persistente delle posizioni o il backend di ricerca.

## Profilo

Il profilo v1 è:

```text
antiskill_position_standard_v1
```

L'identità comprende soltanto lo stato necessario a determinare le mosse legali ordinarie:

- variante `standard`;
- disposizione dei pezzi;
- colore al tratto;
- diritti di arrocco effettivi, ordinati `KQkq`;
- casa en passant soltanto quando una cattura en passant è legalmente disponibile.

Sono esclusi:

- halfmove clock;
- numero della mossa;
- storia delle ripetizioni;
- possibilità di reclamo per patta;
- giocatori, rating, evento, risultato e tempo;
- commenti, annotazioni e valutazioni.

## Normalizzazione

L'input FEN viene prima interpretato come posizione di Scacchi standard. L'identità non dipende dallo spelling originale del FEN.

La chiave semantica leggibile ha quattro campi:

```text
<piece-placement> <side> <castling|-> <legal-ep|->
```

Per esempio la posizione iniziale produce:

```text
rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -
```

Il target en passant presente nel testo FEN ma non legalmente catturabile viene normalizzato a `-`. Halfmove e fullmove non entrano nella chiave.

FEN canonici e FEN recuperabili che descrivono lo stesso stato semantico devono convergere sulla stessa chiave e sullo stesso `position_id`; lo stato `canonical|recovered` resta diagnostica dell'input, non parte dell'identità.

## Hash

`position_id` è il BLAKE3 lowercase esadecimale della preimage versionata:

```text
antiskill_position_standard_v1 NUL
standard NUL
<chiave semantica UTF-8>
```

Cambiare semantica richiede un nuovo profilo/versione: non si reinterpretano retroattivamente gli ID v1.

## Distinzione da rule_state_id

Due occorrenze con lo stesso `position_id` possono avere valore regolamentare differente per contatore delle cinquanta mosse, ripetizioni o altra storia pertinente. Queste informazioni appartengono al futuro `rule_state_id` o all'occorrenza, non vengono introdotte di nascosto in `position_id`.

Di conseguenza una `state_analysis` non può essere riusata automaticamente come analisi universale di tutte le occorrenze dello stesso `position_id`.

## Superficie di verifica

La superficie tecnica v1 è:

```bash
antiskill inspector position-id --fen "FEN"
```

Restituisce profilo, `position_id`, chiave semantica e stato di normalizzazione del FEN. La CLI serve come conformance surface del contratto; non è ancora un indice di corpus.

## Invarianti per il futuro indice

L'indice posizioni deve:

- essere derivato e ricostruibile dalle occurrence verificate;
- mappare `position_id -> occurrence + ply` senza modificare le occurrence;
- poter essere costruito bounded-memory;
- fallire chiuso su Collection o artefatti incompatibili/corrotti;
- restare distinto dallo store delle Analisi;
- non scegliere un backend persistente prima di misure su dimensione e latenza.
