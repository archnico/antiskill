# Invarianti di Antiskill

Queste regole valgono per ogni formato, importatore, indice e vista del progetto.

## Custodia

1. Ogni file acquisito deve essere conservato byte per byte in uno snapshot immutabile.
2. Il percorso originale è metadato, non un requisito per riaprire il database.
3. File e record corrotti vengono messi in quarantena, mai scartati.
4. Reimportare lo stesso artefatto non deve duplicare dati né cambiare lo stato osservabile.

## Provenienza

Ogni valore deve appartenere a uno di questi livelli:

- `raw`: presente nella sorgente acquisita;
- `asserted`: dichiarato da una fonte esterna;
- `derived`: calcolato da antiskill;
- `canonical`: scelto per visualizzazione o esportazione.

Ogni valore non raw deve mantenere fonte, metodo, data e livello di confidenza. L'autorità è contestuale al tipo di affermazione e al periodo, non una precedenza globale tra sorgenti.

## Identità

- `record_id`, `line_id`, `game_id`, `person_id`, `account_id` ed `event_id` sono concetti distinti.
- Un nome testuale non è una persona: è una menzione da risolvere.
- Due identificatori ufficiali incompatibili non possono essere fusi automaticamente.
- Ogni collegamento o fusione deve poter essere annullato senza perdere evidenze.
- Il fuzzy matching da solo non autorizza mai una fusione automatica.

## Partite e annotazioni

- La linea canonica dipende da variante, posizione iniziale e mosse principali.
- Header, risultato dichiarato, commenti e annotazioni non partecipano all'identità della linea.
- Mainline, varianti e sottovarianti formano un albero ordinato.
- Commenti, NAG, valutazioni, orologi e grafica restano elementi distinti.
- Annotazioni provenienti da fonti differenti restano livelli separati; la combinazione è una vista.
- Una valutazione esterna conserva fonte, engine/profilo, profondità/nodi e data quando disponibili; se il metodo è ignoto non è equivalente a un'analisi Antiskill standardizzata.
- Un'analisi già disponibile e sufficientemente qualificata va riusata prima di ricalcolarla, salvo che una ricetta richieda deliberatamente un profilo omogeneo.
- Parse, serializzazione e nuovo parse devono conservare semanticamente l'intero albero.

## Fonti esterne

- Ogni acquisizione produce prima uno snapshot immutabile e poi dati strutturati.
- Devono essere registrati almeno URL o identificatore, istante di acquisizione, hash, parser e classe dei diritti.
- Una variazione inattesa del formato deve interrompere l'importazione strutturata e conservare il raw.
- Nessun connettore deve aggirare autenticazione, limitazioni tecniche o condizioni d'uso.
- I dati con licenze differenti non vengono redistribuiti come un corpus indistinto.

## Prestazioni

- I file grandi vengono elaborati in streaming.
- La memoria non deve crescere linearmente con la dimensione del corpus.
- Le viste utente sono paginate e non caricano mai tutto il database.
- Gli indici sono introdotti soltanto per query misurate e frequenti.
- Le operazioni lunghe devono essere interrompibili e riprendibili.
