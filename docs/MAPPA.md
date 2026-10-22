# Mappa

Antiskill separa il **Nucleo affidabile** dalle applicazioni che possono crescergli attorno.

L'esperimento scientifico che ha dato il nome al progetto vive in **Ricerca**: è importante, ma non definisce il perimetro del software.

Questa è la mappa delle **responsabilità**. La mappa dei prodotti è complementare: una [Base comune](BASE_E_VERTICALI.md) raccoglie capacità condivise e le verticali le combinano fino a un uso concreto. Una verticale può quindi attraversare Nucleo, Fonti, Analisi, Rapporti e Piattaforma senza creare una seconda infrastruttura.

## Aree

| Area | Scopo | Esempi |
|---|---|---|
| **Nucleo** | custodire e interrogare evidenza scacchistica | PGN, tag, provenienza, identità, deduplica, indici, ricerca, export |
| **Ricerca** | esperimenti riproducibili | protocolli, campioni congelati, Maia, statistica, replicazioni |
| **Rapporti** | rispondere a domande pratiche con costo contenuto | giocatore, repertorio, torneo, corpus, copertura fonti |
| **Analisi** | calcolo scacchistico selettivo e riusabile | Stockfish, Syzygy, modelli umani, posizioni critiche |
| **Fonti** | acquisire e riconciliare dati esterni | FIDE, FSI, Lichess, Vesus, Torneionline, broadcast, archivi |
| **Interoperabilità** | collegare Antiskill ad altri strumenti | CLI, JSON/JSONL, PGN, En Croissant, Scid, script |
| **Conoscenza** | aggiungere contesto di riferimento | aperture, eventi, albi d'oro, rating list, dati storici |
| **Piattaforma** | eseguire lavori in modo portabile | job, cache, checkpoint, resume, distribuzione opzionale |
| **Laboratorio** | provare idee senza promettere stabilità | nuovi indici, modelli, formati, prototipi |
| **Qualità** | verificare tutto il resto | test, fixture, fuzzing, benchmark, portabilità, privacy |

## Confini

Il flusso desiderato è semplice:

```text
Fonti ───────┐
Conoscenza ──┼──> Nucleo ──> Rapporti
             │       ├─────> Analisi
             │       ├─────> Interoperabilità
             │       └─────> Ricerca
             │
Piattaforma ─┴─ esegue lavori senza cambiarne la semantica
Laboratorio ─── sperimenta senza diventare dipendenza obbligatoria
Qualità ─────── verifica tutte le aree
```

Il Nucleo deve poter funzionare senza:

- rete;
- Stockfish o Syzygy;
- GUI specifiche;
- orchestratori esterni;
- credenziali;
- corpus privati;
- configurazioni della macchina del maintainer.

## Nucleo

Il Nucleo è conservativo:

- conserva l'evidenza originale dove è promessa gestione lossless;
- separa occorrenza, sequenza canonica e partita logica;
- rende esplicite identità e provenienza;
- diagnostica i duplicati senza cancellarli;
- verifica gli artefatti persistenti fail-closed;
- espone contratti strutturati a Rapporti, Analisi e strumenti esterni.

Il codice corrente vive soprattutto in `src/archive/`, `src/inspector/`, `src/dedup/` e nello storage condiviso.

## Applicazioni e verticali

Le aree non sono un elenco chiuso di prodotti. Una verticale è un percorso end-to-end verso un risultato utilizzabile e può attraversare più aree mantenendo un solo insieme di identità, provenance e contratti condivisi.

Una nuova idea può entrare come:

- funzione del Nucleo;
- adapter di Fonte;
- ricetta di Rapporto o Analisi;
- integrazione esterna;
- prototipo in Laboratorio;
- progetto complementare indipendente.

La collocazione si decide **dopo** aver chiarito valore, dipendenze, dati, costo e manutenzione. Vedi [Possibilità](POSSIBILITA.md).

## Git

Le aree non sono branch permanenti. I branch sono temporanei e focalizzati, per esempio:

```text
nucleo/...
ricerca/...
rapporti/...
analisi/...
fonti/...
interop/...
conoscenza/...
piattaforma/...
laboratorio/...
qualita/...
docs/...
```

Flusso normale:

```text
idea o issue → branch → PR → review → merge → eliminazione branch
```

Per correzioni piccole e ovvie la issue preventiva non è obbligatoria.

## GitHub

Le label di area usano lo stesso vocabolario:

```text
area:nucleo
area:ricerca
area:rapporti
area:analisi
area:fonti
area:interoperabilita
area:conoscenza
area:piattaforma
area:laboratorio
area:qualita
```

La classificazione serve a orientarsi, non a bloccare proposte trasversali o ancora difficili da collocare.
