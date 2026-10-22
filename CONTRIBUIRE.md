# Contribuire ad Antiskill

[English](CONTRIBUTING.md) · [LEGGIMI](LEGGIMI.md)

Antiskill accoglie codice, competenza scacchistica, verifica delle fonti, statistica, testing, documentazione, ricerca storica e idee di integrazione. **Non serve conoscere tutto il progetto né scrivere Rust.**

## Parti dal contributo utile più piccolo

- **Correzione piccola:** apri direttamente una pull request focalizzata.
- **Bug:** apri una segnalazione o una PR con un caso minimo riproducibile.
- **Nuova applicazione o idea:** usa il template Contributo generale; il prototipo è benvenuto ma non obbligatorio.
- **Nuova fonte o adapter:** usa il template Fonte.
- **Nuovo rapporto:** usa il template Rapporto.
- **Formato persistente, regola d'identità, comportamento di rete o architettura ampia:** discutine prima in una issue.

Le issue libere restano abilitate. Se non sai dove classificare una proposta, scrivilo: la classificazione può farla chi mantiene il progetto.

## Aree

| Area | Contributi tipici |
|---|---|
| **Nucleo** | PGN, storage, identità, deduplica, indici, ricerca, export, prestazioni |
| **Ricerca** | protocolli, statistica, replicazioni, modelli di scelta umana |
| **Rapporti** | giocatore, repertorio, torneo, corpus, fonti |
| **Analisi** | Stockfish, Syzygy, UCI, archivio delle analisi |
| **Fonti** | federazioni, sistemi torneo, broadcast, archivi |
| **Interoperabilità** | CLI/formati, En Croissant, Scid, script, API |
| **Conoscenza** | aperture, eventi, dati storici e di riferimento |
| **Piattaforma** | job, cache, checkpoint, resume, esecuzione portabile |
| **Laboratorio** | esperimenti e prototipi delimitati |
| **Qualità** | test, fixture, portabilità, documentazione, privacy |

Vedi [Percorsi dei contributori](docs/CONTRIBUTORI.md), [Mappa](docs/MAPPA.md) e [Base comune e verticali](docs/BASE_E_VERTICALI.md).

Le aree assegnano **ownership**. Lo sviluppo dei prodotti usa la mappa complementare **Base comune + verticali**: se una capacità serve a più applicazioni, il suo contratto stabile entra nella Base sotto l’area che ne possiede la semantica; se serve a un solo uso, resta nella verticale e riusa la Base invece di ricostruire ingestione, identità, deduplica, provenance, query o job.

## Le applicazioni restano aperte

Un contributo non deve appartenere a un elenco di feature già approvato. Ci interessano applicazioni utili di un archivio scacchistico affidabile, comprese idee che non abbiamo previsto.

Dopo una prova concreta, una proposta può diventare:

- parte del Nucleo;
- un adapter di Fonte;
- una ricetta di Rapporto o Analisi;
- un pacchetto di Interoperabilità;
- un prototipo in Laboratorio;
- un progetto complementare indipendente;
- oppure qualcosa che decidiamo di non integrare.

La decisione considera valore, evidenza, costo di manutenzione, privacy, diritti, coerenza architetturale e possibilità di ottenere lo stesso risultato in modo più pulito fuori dal Nucleo.

## Pull request

Mantieni ogni PR focalizzata su un solo problema. Per lavori sostanziali una issue è consigliata, ma non è burocrazia: una correzione piccola e ovvia non deve aspettare un'assegnazione.

Una buona PR spiega:

- cosa cambia e perché;
- come è stata verificata;
- eventuali effetti su dati, privacy, diritti, compatibilità o formati persistenti;
- se introduce capacità condivisa della Base o logica specifica di una verticale, e quali verticali ne beneficiano o quali contratti comuni vengono riusati;
- cosa resta deliberatamente fuori, quando è rilevante.

Non fare push diretto su `master`.

## DCO

Antiskill usa il **Developer Certificate of Origin 1.1**. Firma ogni commit con:

```bash
git commit -s
```

Se hai dimenticato la firma nell'ultimo commit:

```bash
git commit --amend -s --no-edit
```

La firma certifica che hai diritto di inviare il contributo con la licenza del progetto. Ogni contributor mantiene il copyright sul proprio lavoro.

## Controlli locali

Per modifiche Rust, i controlli normali prima della PR sono:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --locked
```

Per modifiche solo documentali non è necessario eseguire localmente l'intera suite Rust; la CI continuerà comunque a controllare il repository.

## Test e dati

Preferisci fixture deterministiche, minime e sintetiche. Non caricare corpus privati, credenziali, dati personali non necessari, dettagli specifici delle macchine o grandi copie di materiale di terzi con diritti di redistribuzione incerti.

Quando serve un caso reale, riducilo al minimo esempio redistribuibile oppure conserva l'evidenza raw fuori dal repository pubblico e documenta solo ciò che serve per riprodurre il comportamento in sicurezza.

I parser delle fonti devono fallire chiaramente su input incoerenti o non supportati invece di inventare dati mancanti. Le modifiche a persistenza e identità devono mantenere provenienza esplicita e verifica fail-closed.

Vedi [Privacy](docs/PRIVACY.md), [Invarianti](docs/INVARIANTS.md) e [Fonti](docs/SOURCES.md).

## Documentazione

`README.md` e `LEGGIMI.md` devono restare equivalenti nella sostanza, così come `CONTRIBUTING.md` e `CONTRIBUIRE.md`. L’italiano è la lingua di lavoro per la documentazione che evolve rapidamente; gli ingressi in inglese sono mantenuti al meglio e un inglese non ancora rifinito **non blocca** pubblicazione o merge. La documentazione vicina al codice può usare una sola lingua quando duplicarla aumenta la manutenzione più della chiarezza.

Usa in modo coerente i nomi brevi: **Nucleo, Ricerca, Rapporti, Analisi, Fonti, Interoperabilità, Conoscenza, Piattaforma, Laboratorio, Qualità**.

## Revisione

La CI verde è necessaria ma non sufficiente. I maintainer possono chiedere un confine più piccolo, evidenze migliori, test sintetici, diritti più chiari o un diverso punto di integrazione.

La revisione può anche concludere che un'idea è valida ma sta meglio fuori dal repository principale. Non è un contributo fallito: mantenere piccolo il Nucleo è parte del progetto.

## Licenza

Il software Antiskill usa **EUPL-1.2**. La licenza del software non rilicenzia automaticamente i dati scacchistici di terzi.
