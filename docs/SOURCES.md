# Registro delle fonti

Questo documento descrive le fonti candidate per l'aggiornamento automatico di `antiskill`. Il registro eseguibile iniziale vive in `src/sources/mod.rs`; questo file conserva motivazioni, diritti e strategia.

La presenza in questo elenco **non implica che l'adapter sia già implementato o completo**. Il codice e il registro eseguibile indicano il supporto effettivo; qui convivono fonti attive, in sviluppo e candidate.

## Regole comuni

Per ogni acquisizione registrare:

```text
source_key
source_family
source_url_or_external_id
fetched_at
published_at
etag
last_modified
raw_hash
parser_version
rights_class
snapshot_id
```

Classi dei diritti:

- `open`: dati con licenza aperta esplicita;
- `attribution`: riuso con attribuzione o share-alike;
- `internal`: acquisizione locale, senza redistribuzione automatica;
- `licensed`: importazione soltanto con licenza o autorizzazione specifica.

Lo snapshot raw viene conservato prima del parsing. Un errore di formato non deve eliminare né sostituire lo snapshot. Le cadenze dichiarate sono obiettivi operativi per Job ripetibili: ogni controllo produce evidenza di freschezza, mentre i cambiamenti generano nuovi snapshot e diff senza riscrivere la storia.

L'autorità indicata è sempre **contestuale**: FIDE può essere primaria per un rating FIDE, FSI per il tesseramento italiano e un organizzatore per il risultato di uno specifico evento. La riconciliazione conserva il perché della scelta invece di definire una graduatoria universale delle fonti.

# Spina dorsale italiana

## FIDE rating lists

- Chiave: `fide-rating-list`
- Pagina ufficiale: <https://ratings.fide.com/download_lists.phtml>
- Cadenza: mensile.
- Formati: TXT e XML.
- Utilità: FIDE ID, nome, federazione, titoli, sesso, anno di nascita, rating standard/rapid/blitz e relativi dati tecnici.
- Autorità: molto alta per identità FIDE e rating mensili.
- Strategia: snapshot immutabile di ogni mese; mai sovrascrivere le liste passate.
- Diritti: `internal` finché non viene verificata una licenza esplicita per la redistribuzione.

## FSI tesserati

- Chiave: `fsi-members`
- Pagina ufficiale: <https://www.federscacchi.com/fsi/index.php/struttura/tesserati>
- Cadenza proposta: giornaliera per l'anno corrente.
- Utilità: ID FSI, nome, sesso, nascita, categoria, cittadinanza, società, codice società e tipo di tessera.
- Accesso preferito: esportazione CSV ufficiale quando disponibile; HTML soltanto come fallback verificato.
- Autorità: molto alta per lo stato del tesseramento italiano.
- Strategia: snapshot annuali e delta giornalieri; segmentare le interrogazioni se la vista impone limiti.
- Diritti: `internal`.

## FSI persone

- Chiave: `fsi-people`
- Famiglia: anagrafe storica delle persone già tesserate.
- Cadenza proposta: mensile e su richiesta durante la risoluzione di identità.
- Utilità: recupero di persone non presenti nel tesseramento corrente.
- Limiti: la ricerca può essere parziale e limitata nel numero di risultati.
- Strategia: scansione segmentata e tracciamento della copertura; non considerare l'assenza come prova di inesistenza.
- Diritti: `internal`.

## FSI società

- Chiave: `fsi-clubs`
- Pagina ufficiale: <https://www.federscacchi.com/fsi/index.php/struttura/societa>
- Cadenza proposta: settimanale; più frequente durante l'affiliazione annuale.
- Utilità: codice società, denominazione, sede, contatti, presidente, consiglio e stato dell'affiliazione.
- Strategia: conservare una versione per anno per ricostruire denominazioni, sedi e incarichi nel tempo.
- Autorità: molto alta.
- Diritti: `internal`.

## FSI calendario

- Chiave: `fsi-calendar`
- Pagina ufficiale: <https://www.federscacchi.com/fsi/index.php/calendario/calendario>
- Cadenza proposta: ogni sei ore.
- Utilità: evento, luogo, provincia, regione, date, organizzatore, contatti, cadenza, sezioni, turni, arbitri, bando e collegamenti esterni.
- Strategia: snapshot delle schede e degli allegati; usare l'identificatore sorgente FSI come chiave esterna, non come `event_id` canonico.
- Autorità: molto alta per annunci e calendario federale.
- Diritti: `internal`.

## FSI albi e struttura

- Chiavi: `fsi-arbiters`, `fsi-instructors`, `fsi-offices`
- Cadenza proposta: settimanale.
- Utilità: categorie e periodi di attività di arbitri e istruttori; composizione degli organi federali e regionali.
- Modello: ruoli temporali collegati a `person_id`, mai campi permanenti della persona.
- Diritti: `internal`.

## FSI documenti e notizie

- Chiavi: `fsi-documents`, `fsi-news`
- Cadenza proposta: ogni 6-12 ore.
- Utilità: regolamenti, verbali, delibere, bilanci, modulistica, comunicati e decisioni.
- Strategia: conservare file originali, hash, data, categoria, organo emittente e relazioni tra versioni.
- Diritti: `internal`.

## FSI archivio partite

- Chiave: `fsi-game-archive`
- Cadenza proposta: giornaliera.
- Utilità: PGN italiani e collegamenti a eventi federali.
- Strategia: conservare il file originale, collegarlo a FSI, Torneionline, Vesus e LOTO quando possibile, poi sottoporlo alla normale ingestione lossless.
- Diritti: `internal` finché non viene verificata una licenza più permissiva.

## Torneionline `allin.csv`

- Chiave: `torneionline-allin`
- File: <https://www.torneionline.com/dwn/allin.csv>
- Manuale: <https://www.torneionline.com/allin.pdf>
- Cadenza: giornaliera.
- Utilità: ponte operativo tra Elo Italia, anagrafica FSI e dati FIDE; include ID FSI e FIDE, rating, nascita, residenza, società e tessere quando disponibili.
- Autorità: alta come tabella di collegamento, non unica fonte canonica.
- Limiti: record parziali, collegamenti mancanti o ritardati e dati provenienti da tabelle differenti.
- Strategia: snapshot quotidiano; preferire FIDE per rating FIDE e FSI per tesseramento e società correnti.
- Diritti: `internal`.

## Torneionline T'Pol

- Chiave: `torneionline-tpol`
- Cadenza proposta: giornaliera; ogni 1-2 ore per eventi in corso già noti.
- Utilità: tornei italiani moderni, partecipanti, classifiche, accoppiamenti, risultati, arbitri, performance e variazioni Elo.
- Strategia: importazione per evento con checkpoint; riconciliazione con calendario FSI e PGN.
- Diritti: `internal`.

## Torneionline LOTO

- Chiave: `torneionline-loto`
- Cadenza proposta: settimanale.
- Utilità: tornei storici italiani, fonti, classifiche, tabelloni, albi d'oro e collegamenti a PGN.
- Strategia: catalogare rapidamente tutte le schede e conservare localmente gli artefatti storici; parsing approfondito progressivo.
- Autorità: alta come archivio storico curato, da confrontare con le fonti citate in ogni scheda.
- Diritti: `internal`.

## Vesus collegato

- Chiave: `vesus-linked-events`
- Cadenza proposta: ogni 6-12 ore soltanto per eventi scoperti tramite fonti ufficiali o inseriti manualmente.
- Utilità: sezioni, preiscrizioni, categorie, stato delle iscrizioni e informazioni organizzative.
- Strategia: connettore mirato; niente scansione globale finché API, export e condizioni d'uso non sono verificati.
- Diritti: `internal`.

# Fonti internazionali prioritarie

## Lichess database

- Chiavi: `lichess-standard-monthly`, `lichess-broadcast-monthly`
- Indici: <https://database.lichess.org/standard/list.txt> e <https://database.lichess.org/broadcast/list.txt>
- Cadenza: mensile.
- Formati: PGN compressi con Zstandard e checksum.
- Diritti: database principale `open`; broadcast `attribution`.
- Strategia: append-only per mese, verifica checksum e separazione fisica tra partite normali e broadcast.

## Lichess evaluations database

- Chiave: `lichess-evals`.
- Pagina ufficiale: <https://database.lichess.org/>.
- Formato: JSONL compresso Zstandard, indicizzato per FEN.
- Utilità: valutazioni Stockfish e linee principali già calcolate per un grande insieme di posizioni; prima scelta per aumentare copertura evitando compute locale inutile.
- Qualità: profondità, nodi e numero di PV possono variare; conservare sempre i metadati disponibili e non trattare il dataset come un profilo engine uniforme. L'adapter preserva i punteggi nella prospettiva del Bianco usata dall'eval cache Lichess.
- Strategia: snapshot delle release, indice locale per posizione e cache delle risposte usate; le analisi importate restano analisi di fonte distinte dalle run Antiskill standardizzate.
- Adapter v1: `antiskill inspector store-lichess-eval-record` importa selettivamente un singolo record JSON del dump nello Analysis Store, conserva tutte le valutazioni/PV presenti e lega il record allo `snapshot_id` dichiarato. Non è il percorso bulk: il dump completo richiede pack/chunk e indice dedicati per non creare centinaia di milioni di piccoli file.
- Diritti: `open` (CC0 secondo la pagina ufficiale del database Lichess).

## Lichess cloud evaluation

- Chiave candidata: `lichess-cloud-eval`.
- Utilità: lookup selettivo di posizioni mancanti dal corpus locale.
- Strategia: cache persistente, rate limit rigoroso e preferenza per il dump bulk quando applicabile; non usarlo come dipendenza necessaria del Nucleo.
- Provenienza: conservare risposta raw, istante, FEN richiesta e metadati dell'eval disponibili.

## Lichess API

- Chiave: `lichess-api`
- Utilità: account osservati, rating history, attività, tornei, Swiss, broadcast e TRF.
- Strategia: richieste seriali, cache, checkpoint e rispetto rigoroso dei rate limit; non sostituisce i dump storici.
- Diritti: dipendono dall'endpoint e dal contenuto acquisito.

## TWIC

- Chiave: `twic-weekly`
- Pagina: <https://theweekinchess.com/twic>
- Cadenza: settimanale.
- Utilità: pacchetti PGN OTB curati, etichette di eventi e copertura internazionale.
- Diritti: `internal`; uso personale, niente redistribuzione automatica del corpus.

## Chess.com PubAPI

- Chiave: `chesscom-watched-accounts`
- Cadenza proposta: ogni 12-24 ore per account osservati.
- Utilità: ID stabile del giocatore, profilo e archivi mensili delle partite.
- Strategia: conditional GET, mesi nuovi soltanto, niente crawling globale.
- Diritti: `internal` e rispetto delle condizioni della PubAPI.

# Fonti differite

- Chess-Results: importazione mirata per eventi mancanti, non crawling generale.
- Chessgames: riferimento storico e manuale; bulk soltanto con diritti adeguati.
- ChessBase Big/Mega: importazione manuale da copia regolarmente licenziata.
- Siti dei comitati regionali: connettori specifici per calendari, qualificazioni e documenti non presenti nella fonte centrale.
- LiveChessCloud, Tornelo e DGT: connettori live soltanto dopo la costruzione del nucleo di snapshot e riconciliazione.

# Aggiornamento e rilevatore delle lacune

Le fonti con cadenza nota dovrebbero convergere su una primitive comune `sync/check → snapshot → parse → diff → reconcile → index`. Adapter diversi possono avere politiche di rete differenti, ma non devono inventare meccanismi incompatibili di freshness, checkpoint o resume.

Il sistema deve poter distinguere almeno: fonte aggiornata senza cambiamenti, nuovi byte da processare, parser incompatibile, riconciliazione incompleta e fonte scaduta rispetto alla propria cadenza.

## Lacune strutturate

Il sistema deve produrre avvisi strutturati, tra cui:

- evento FSI iniziato senza codice Torneionline;
- evento concluso senza classifica o PGN;
- differenze tra sezioni FSI, Vesus e Torneionline;
- persona con ID FSI ma senza FIDE ID;
- collegamento FSI-FIDE cambiato rispetto allo snapshot precedente;
- nuovo tesserato non ancora presente in `allin.csv`;
- partita italiana senza evento riconosciuto;
- società con denominazione, sede o dirigenti modificati;
- nuovo regolamento che sostituisce una versione precedente;
- posizione rilevante priva di qualunque valutazione riusabile;
- eval esterna presente ma con profilo insufficiente per una ricetta che richiede analisi omogenea;
- albo o serie storica con anni/edizioni mancanti;
- fonte oltre la propria soglia di freschezza.
