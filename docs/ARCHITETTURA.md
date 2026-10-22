# Architettura

## Principi

Antiskill separa quattro cose che non devono confondersi:

1. **evidenza** — ciò che è stato acquisito;
2. **derivazioni** — identità, indici, collegamenti e normalizzazioni controllate;
3. **analisi** — calcolo scacchistico opzionale;
4. **ricerca** — esperimenti scientifici su input congelati.

La direzione è sempre dall'evidenza verso le interpretazioni, mai il contrario.

## Base comune e verticali

Il Nucleo è il cuore semantico di una **Base comune** più ampia. La Base comprende le capacità stabili che più verticali devono poter riusare: custodia, identità, query/indici, joins di Fonti e Conoscenza, analisi riusabili, contratti macchina, Job/Run e qualità. Queste capacità restano possedute dalle rispettive aree; non vengono tutte spostate nel Nucleo.

Le verticali sono consumatori end-to-end della Base. Possono aggiungere logica specifica, ma non devono reimplementare ingestione, deduplica, provenance, identità o orchestrazione per conto proprio. Vedi [Base comune e verticali](BASE_E_VERTICALI.md).

## Corpus verificato e arricchito

Il risultato cumulativo della Base è un corpus interrogabile nel quale evidenza, identità, asserzioni delle fonti, annotazioni, derivazioni e analisi restano separabili. Completezza e freschezza sono proprietà misurate per dimensione, non un'etichetta binaria. Vedi [Corpus verificato e arricchito](CORPUS_VERIFICATO.md).

Una valutazione Stockfish ricevuta da una fonte esterna non diventa automaticamente un'analisi Antiskill: conserva la propria provenienza e il proprio profilo noto o ignoto. La Base preferisce riusare lavoro affidabile già disponibile; il calcolo locale serve a colmare lacune o a produrre profili omogenei richiesti da una ricetta.

## Nucleo

Il Nucleo è il riferimento semantico del progetto. Deve essere:

- locale;
- deterministico;
- lossless dove promesso;
- bounded-memory sui corpus grandi;
- verificabile fail-closed;
- indipendente da rete, GUI, engine e infrastrutture personali.

Occurrence, sequenza canonica e partita logica sono identità diverse e non vanno fuse implicitamente.

## Fonti

Gli adapter possiedono l'accesso al mondo esterno. Un flusso sano separa:

```text
acquisizione → evidenza/snapshot → parsing → normalizzazione → riconciliazione → Nucleo
```

Rete, autenticazione e peculiarità di un sito restano negli adapter. Il Nucleo riceve dati e provenienza attraverso contratti verificabili.

## Rapporti

Un Rapporto è una ricetta versionata che dichiara:

- input richiesti;
- costo atteso;
- bisogno eventuale di Analisi;
- output;
- versione.

Per impostazione predefinita i Rapporti cercano valore nei dati già presenti prima di usare un engine.

## Analisi

L'Analisi è opzionale, selettiva e riusabile. Un risultato deve essere attribuibile almeno a:

```text
soggetto
+ motore o modello
+ build o pesi
+ opzioni
+ profilo
+ limiti di calcolo
= artefatto di analisi
```

Una nuova analisi non cancella quella precedente. La “migliore disponibile” è una vista derivata, non una sovrascrittura.

Syzygy va trattato come informazione esatta distinta da una valutazione euristica di Stockfish.

## Ricerca

La Ricerca consuma viste congelate del Nucleo e artefatti di Analisi identificati.

```text
Nucleo
→ vista congelata
→ campione congelato
→ Analisi congelata
→ risultati
→ statistica
→ pubblicazione
```

Un esito scientifico non può cambiare retroattivamente identità, filtri o semantiche del Nucleo.

## Conoscenza

Aperture, autorità di eventi, albi, rating list e dati storici sono dati di riferimento con provenienza propria. Possono arricchire una vista, ma non riscrivere silenziosamente il raw acquisito. L'autorità è contestuale al campo e al periodo: una vista canonica deve poter spiegare perché ha preferito una fonte a un'altra.

## Interoperabilità

Antiskill espone contratti; non incorpora una GUI come dipendenza.

Ordine preferito:

```text
API interne strutturate → CLI / JSON / JSONL / PGN → strumenti esterni
```

Uno strumento esterno importante può influenzare il design dei contratti, ma non possederne la semantica.

## Piattaforma

L'esecuzione deve restare separata dal significato del lavoro. Un job dovrebbe poter girare localmente o su un esecutore distribuito senza cambiare risultato o identità scientifica.

Schema concettuale:

```text
Job
- input
- operazione e profilo
- parametri
- risorse suggerite
- checkpoint
- output attesi

Run
- stato
- progresso
- errori
- artefatti
- manifest finale
```

Locale è il riferimento; distribuzione, cache e resume sono acceleratori opzionali.

## Laboratorio

Il Laboratorio può rompere compatibilità, cambiare modello dati e provare idee velocemente. Non può però diventare una dipendenza obbligatoria del Nucleo senza una promozione esplicita.

Una promozione richiede almeno:

- problema reale;
- contratto chiaro;
- test;
- uno strumento che la utilizzi o un uso concreto;
- costo di manutenzione accettabile.

## Qualità

Qualità attraversa ogni area e include:

- test e fixture sintetiche;
- fuzzing e casi ostili;
- benchmark;
- portabilità;
- controllo privacy e diritti;
- verifica dei link e della documentazione;
- gate di pubblicazione.

## Moduli attuali

| Percorso | Area primaria |
|---|---|
| `src/archive/` | Nucleo / CLI stabile |
| `src/inspector/` | Nucleo / pipeline lossless |
| `src/dedup/` | Nucleo |
| `src/object_store.rs` | Nucleo |
| `src/players.rs` | Nucleo con confini verso Fonti e Conoscenza |
| `src/sources/` | Fonti |
| `src/lab/` | Laboratorio |
| `docs/research/` | Ricerca |

Gli spostamenti fisici del codice seguono i confini già dimostrati dai test; non li anticipano.
