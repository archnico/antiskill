# Antiskill

[English](README.en.md) · [Contribuire](CONTRIBUIRE.md) · [Licenza](LICENSE)

**Antiskill è un'infrastruttura locale e verificabile per archivi scacchistici, consultazione, rapporti, analisi e ricerca scientifica riproducibile.**

Trasforma file PGN e fonti scacchistiche esterne in un archivio con provenienza esplicita, interrogabile e arricchibile senza perdere l'evidenza originale.

**Antiskill distribuisce software, non un database centrale.** Ogni utente costruisce il proprio archivio locale usando dati che ha diritto di utilizzare.

## Perché

I dati scacchistici sono utili ma disordinati: la stessa partita compare più volte, i tag discordano, le identità cambiano, le fonti scompaiono e le analisi engine su larga scala costano.

Antiskill separa questi problemi e vuole rendere semplice:

- conservare evidenza e provenienza;
- trovare partite, giocatori, eventi, posizioni e duplicati;
- produrre rapporti utili prima di spendere calcolo engine;
- riusare analisi Stockfish, Syzygy o di modelli invece di ricalcolarle;
- combinare fonti federali, di torneo, broadcast, storiche e personali;
- eseguire esperimenti scientifici su input congelati e verificabili;
- collegare strumenti esterni senza trasformare una GUI nel Nucleo.

L'esperimento scientifico “Antiskill” è un'applicazione importante, non il confine del software.

Un modo utile di leggere il progetto è **una Base comune da cui partono molte verticali**. La Base contiene capacità riusate da più applicazioni; ogni verticale attraversa le aree necessarie e termina in un rapporto, dossier, esperimento o integrazione utilizzabile. Le dieci aree restano confini di responsabilità, non stack di prodotto concorrenti. Vedi [Base comune e verticali](docs/BASE_E_VERTICALI.md).

## Oggi

Il percorso più maturo è l'archivio PGN locale:

```text
init → ingest → doctor → compare/search/duplicates/export
```

| Comando | Scopo |
|---|---|
| `init` | crea un workspace |
| `ingest` | acquisisce lo snapshot corrente delle sorgenti PGN |
| `doctor` | verifica l'archivio attivo senza modificarlo |
| `compare` | confronta una directory con lo snapshot attivo |
| `search` | cerca per giocatore, evento e data |
| `duplicates` | diagnostica duplicati esatti e canonici senza distruggere dati |
| `export` | recupera occorrenze PGN originali byte-esatte |

`antiskill search --format json` espone il contratto strutturato e versionato `antiskill.search.v1` per script e strumenti esterni.

Integrazioni di fonti, rapporti avanzati, archivio delle analisi, indice posizioni e strumenti di Ricerca sono aree in sviluppo e possono essere meno stabili del Nucleo.

## Possibilità

Antiskill mantiene volutamente ampio il ventaglio delle applicazioni. Una buona proposta può diventare parte del **Nucleo**, un adapter, una ricetta di rapporto, uno strato di interoperabilità, un prototipo in **Laboratorio** o un progetto complementare indipendente.

Esempi: preparazione giocatore, repertori, completezza di tornei e broadcast, ricerca storica, riconciliazione federale, ricerca per posizione e trasposizione, strumenti didattici, qualità dei corpus, revisione assistita da engine, tablebase, studi scientifici e integrazioni con En Croissant o Scid.

Sono possibilità, non promesse. Preferiamo vedere contributi piccoli e funzionanti e poi decidere il confine migliore. Vedi [Possibilità](docs/POSSIBILITA.md).

## Aree

Il progetto usa dieci nomi italiani brevi come vocabolario stabile:

| Area | Responsabilità |
|---|---|
| **Nucleo** | PGN, provenienza, identità, deduplica, indici, ricerca, export esatto |
| **Ricerca** | protocolli scientifici, campioni congelati, statistica, replicazione |
| **Rapporti** | sintesi e dossier utili a basso costo |
| **Analisi** | Stockfish, Syzygy, modelli umani, analisi riusabile |
| **Fonti** | FIDE, FSI, Lichess, sistemi torneo, broadcast, archivi |
| **Interoperabilità** | CLI, JSON/JSONL, PGN, strumenti esterni |
| **Conoscenza** | aperture, autorità eventi, dati storici e di riferimento |
| **Piattaforma** | job, cache, checkpoint, resume, distribuzione opzionale |
| **Laboratorio** | prototipi senza promessa di stabilità |
| **Qualità** | test, fixture, portabilità, privacy, sicurezza della pubblicazione |

Le aree sono confini architetturali, **non branch Git permanenti**. Vedi [Mappa](docs/MAPPA.md) e [Architettura](docs/ARCHITETTURA.md).

## Avvio rapido

Da un checkout con Rust e Cargo:

```bash
cargo install --path . --locked
antiskill --version

antiskill init archivio
cd archivio
antiskill ingest /percorso/ai/pgn
antiskill doctor
antiskill search --player "Kasparov"
antiskill duplicates
```

`antiskill <COMANDO> --help` è il riferimento autorevole per la sintassi.

## Garanzie

Il Nucleo è conservativo per scelta:

- i byte PGN originali restano recuperabili dove è promessa gestione lossless;
- la provenienza viene conservata negli artefatti derivati;
- le catene persistenti vengono verificate fail-closed prima dell'uso;
- la deduplica non cancella o fonde silenziosamente l'evidenza;
- i comandi diagnostici non riparano dati alle spalle dell'utente;
- l'export esatto restituisce l'occorrenza originale invece di rigenerare il PGN.

## Fonti, diritti e privacy

Il sorgente Antiskill usa **EUPL-1.2**. L’uso commerciale è consentito; contributi e modifiche seguono la licenza del progetto, mentre software indipendente costruito attorno ad Antiskill può mantenere una propria licenza. I dati importati o acquisiti mantengono i propri diritti e vincoli.

La regola è: **specializzazione italiana, architettura internazionale**. Il progetto può mantenere direttamente FIDE/FSI mentre adapter comunitari possono coprire altre federazioni e sistemi.

I test pubblici usano di norma piccole fixture sintetiche. Corpus privati, identificativi personali non necessari, credenziali, dettagli delle macchine locali e materiale con redistribuzione incerta non appartengono al repository pubblico. Vedi [Privacy](docs/PRIVACY.md) e [Fonti](docs/SOURCES.md).

## Contribuire

Non serve scrivere Rust. Conoscenza scacchistica, verifica delle fonti, statistica, testing, documentazione, qualità dati, ricerca storica, integrazioni UI e casi di bug riproducibili sono contributi utili.

Per una correzione piccola e ovvia va bene aprire direttamente una pull request focalizzata. Per una nuova applicazione, fonte, formato persistente, regola d'identità o modifica architetturale ampia è meglio aprire prima una issue: discutere il confine costa molto meno che riscrivere codice.

Le idee sono benvenute anche se non rientrano ancora nel piano. Una proposta può essere integrata in Antiskill, incubata in **Laboratorio**, mantenuta come integrazione esterna oppure non accolta dopo la revisione; l'importante è rendere verificabili valore e confine.

Vedi [Contribuire](CONTRIBUIRE.md) e [Percorsi dei contributori](docs/CONTRIBUTORI.md). I commit delle pull request usano il Developer Certificate of Origin 1.1 (`git commit -s`).

## Documentazione

- [Presentazione](docs/PRESENTAZIONE.md)
- [Mappa](docs/MAPPA.md)
- [Base comune e verticali](docs/BASE_E_VERTICALI.md)
- [Architettura](docs/ARCHITETTURA.md)
- [Possibilità](docs/POSSIBILITA.md)
- [Rapporti](docs/RAPPORTI.md)
- [Fonti](docs/SOURCES.md)
- [Privacy](docs/PRIVACY.md)
- [Sicurezza](SECURITY.md)
- [Licenze](docs/LICENZE.md)
- [Piano](docs/ROADMAP.md)
- [Indice](docs/INDICE.md)

I documenti molto vicini all'implementazione restano fuori dal percorso principale quando non aiutano un contributore a fare una modifica concreta.

## Stato

Antiskill è in sviluppo attivo. Il workflow PGN del Nucleo è oggi la superficie pubblica più solida. Non sono richiesti GUI, servizio centrale o engine; copertura delle fonti, Analisi, Rapporti e Ricerca restano aree aperte a crescita prudente.
