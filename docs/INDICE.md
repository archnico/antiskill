# Indice

## Inizia qui

- [Presentazione](PRESENTAZIONE.md) — cos'è Antiskill e perché esiste;
- [Mappa](MAPPA.md) — aree e confini;
- [Base comune e verticali](BASE_E_VERTICALI.md) — capacità condivise e usi end-to-end;
- [Corpus verificato e arricchito](CORPUS_VERIFICATO.md) — strati, autorità, copertura, analisi e aggiornamento;
- [Possibilità](POSSIBILITA.md) — applicazioni aperte alla sperimentazione;
- [Piano](ROADMAP.md) — priorità correnti;
- [Contributori](CONTRIBUTORI.md) — modi semplici per collaborare;
- [Privacy](PRIVACY.md) — cosa può entrare nel repository pubblico;
- [Sicurezza](../SECURITY.md) — segnalazioni sensibili e bug non sensibili;
- [Licenze](LICENZE.md) — sorgente, dipendenze e distribuzione degli artefatti.

## Architettura

- [Architettura](ARCHITETTURA.md) — dipendenze e principi;
- [Separazione](SEPARAZIONE.md) — confini fisici del codice;
- [Invarianti](INVARIANTS.md) — conservazione, provenienza e identità;
- [Inspector](INSPECTOR_ENGINE_V0.md) — pipeline PGN;
- [Snapshot](SNAPSHOT_FORMAT.md) — formato di acquisizione;
- [Store delle analisi](ANALYSIS_STORE.md) — posizioni, provenance, riuso e profili di analisi;
- [Identità posizione](POSITION_IDENTITY.md) — chiave canonica `position_id` per Scacchi standard;
- [Decisioni dipendenze](DEPENDENCY_DECISIONS.md).

## Prodotti

- [Rapporti](RAPPORTI.md);
- [Prestazioni](PERFORMANCE.md);
- [Baseline prestazioni](PERFORMANCE_BASELINE_V1.md).

## Fonti

- [Registro delle fonti](SOURCES.md).

## Ricerca

- [Quadro delle formulazioni](research/QUADRO.md) — domande, baseline, archivi, risposte dei dati e stato;
- `research/pilot-esplorativo-v1.md` — pilot esplorativo pubblico anonimizzato;
- `research/main-study-protocol-v1.md` — protocollo dello studio principale.

Corpus, run raw e identità non necessarie non fanno automaticamente parte della documentazione pubblica.

## Storia tecnica

I documenti `issue-*-acceptance-*.md` e altri report datati registrano milestone tecniche. Sono utili per audit e sviluppo, ma non costituiscono il percorso introduttivo del progetto.

I numeri di issue citati in questi report sono identificatori del repository di sviluppo storico: non sono link alle issue del GitHub pubblico.

## Laboratorio

- `theory-spec-v1.md`;
- `theory-build-status-20260728.md`.

Il materiale di Laboratorio è sperimentale finché non viene promosso con contratto e test espliciti.

## Pubblicazione

- [Pubblicazione](PUBBLICAZIONE.md) — criteri del repository pubblico;
- workflow CI sotto `.github/workflows/`;
- fixture sotto `tests/fixtures/`;
- `tools/check-public-tree.sh`.

## Regola

La Mappa definisce l'ownership concettuale. I path storici possono restare dove sono finché spostarli non riduce davvero complessità o ambiguità.
