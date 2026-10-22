# Antiskill

[Italiano](README.md) · [Contributing](CONTRIBUTING.md) · [License](LICENSE)

**Antiskill is local, verifiable infrastructure for chess archives, search, reports, analysis, and reproducible research.**

It turns PGN files and external chess sources into a provenance-aware archive that can be queried, reconciled, exported, enriched, and analysed without losing the original evidence.

**Antiskill ships software, not a central database.** Users build local archives from data they are entitled to use.

## Why

Chess data is useful but messy: the same game can appear several times, tags disagree, identities drift, sources disappear, and large engine runs are expensive. Antiskill separates these problems instead of hiding them behind one opaque database.

The project aims to make it easy to:

- preserve source evidence and provenance;
- find games, players, events, positions, and duplicates;
- reconcile federation, tournament, broadcast, historical, and user sources into one verified corpus;
- reuse trustworthy source-provided evaluations and annotations before spending local engine time;
- reuse Stockfish, Syzygy, or model analysis instead of recomputing it;
- run scientific experiments on frozen, auditable inputs;
- connect external tools without making a GUI part of the core.

The scientific “Antiskill” experiment is one important application, not the boundary of the software.

A useful way to picture the project is **one shared base with many vertical slices**. The base contains capabilities reused by several applications; each vertical crosses the relevant areas and ends in a usable report, dossier, experiment, or integration. The long-term data product underneath them is a [verified and enriched corpus](docs/CORPUS_VERIFICATO.md): evidence, identities, source assertions, annotations and reusable analyses that remain attributable instead of being flattened into one opaque database. The ten areas below remain ownership boundaries, not competing product stacks. See [Shared base and verticals](docs/BASE_E_VERTICALI.md).

## Today

The most mature path is the local PGN archive:

```text
init → ingest → doctor → compare/search/duplicates/export
```

| Command | Purpose |
|---|---|
| `init` | create a workspace |
| `ingest` | ingest the current PGN source snapshot |
| `doctor` | verify the active archive without modifying it |
| `compare` | compare a directory with the active snapshot |
| `search` | query games by player, event, and date |
| `duplicates` | diagnose exact and canonical duplicates, non-destructively |
| `export` | recover original byte-exact PGN occurrences |

`antiskill search --format json` exposes the versioned `antiskill.search.v1` machine-readable contract for scripts and external consumers.

Source integrations, advanced reports, analysis storage, position indexing, and research tooling are active development areas and may be less stable than the archive path.

## Possibilities

Antiskill intentionally keeps a wide application surface. Useful proposals may become part of the **Nucleo**, an adapter, a report recipe, an interoperability layer, a **Laboratorio** prototype, or an independent companion project.

Examples include player preparation, repertoire studies, tournament and broadcast completeness, historical chess research, federation reconciliation, position/transposition search, teaching tools, corpus quality control, engine-assisted review, tablebase workflows, scientific studies, and integrations with tools such as En Croissant or Scid.

These are possibilities, not promises. We prefer to see small working contributions and then decide the cleanest long-term boundary. See [Possibilities](docs/POSSIBILITA.md).

## Areas

The project uses ten short Italian area names as stable vocabulary:

| Area | Responsibility |
|---|---|
| **Nucleo** | PGN, provenance, identity, deduplication, indexes, search, exact export |
| **Ricerca** | scientific protocols, frozen samples, statistics, replication |
| **Rapporti** | useful low-cost summaries and dossiers |
| **Analisi** | Stockfish, Syzygy, human models, reusable analysis |
| **Fonti** | FIDE, FSI, Lichess, tournament systems, broadcasts, archives |
| **Interoperabilità** | CLI, JSON/JSONL, PGN, external consumers |
| **Conoscenza** | openings, event authority, historical and reference data |
| **Piattaforma** | jobs, cache, checkpoint, resume, optional distribution |
| **Laboratorio** | prototypes without stability promises |
| **Qualità** | tests, fixtures, portability, privacy, publication safety |

Areas are architectural boundaries, **not permanent Git branches**. See the [Project map](docs/MAPPA.md) and [Architecture](docs/ARCHITETTURA.md).

## Quick start

From a checkout with Rust and Cargo installed:

```bash
cargo install --path . --locked
antiskill --version

antiskill init archive
cd archive
antiskill ingest /path/to/pgn
antiskill doctor
antiskill search --player "Kasparov"
antiskill duplicates
```

Use `antiskill <COMMAND> --help` as the authoritative syntax reference.

## Guarantees

The Nucleo is conservative by design:

- original PGN bytes remain recoverable where lossless handling is promised;
- provenance is retained through derived artifacts;
- persisted chains are verified fail-closed before use;
- duplicate detection does not silently delete or merge evidence;
- diagnostics do not repair data behind the user's back;
- exact export returns the original occurrence rather than regenerated PGN.

## Sources, rights, and privacy

Antiskill source code is licensed under **EUPL-1.2**. Commercial use is allowed; contributions and modifications follow the project licence, while independent surrounding software may keep its own licence. Imported or acquired data keeps its own rights and restrictions.

The project follows a simple rule: **Italian specialization, international architecture.** FIDE/FSI support can be maintained centrally while community adapters can cover other federations and systems.

Public tests should use small synthetic fixtures by default. Private corpora, unnecessary personal identifiers, credentials, local machine details, and material with unclear redistribution rights do not belong in the public repository. See [Privacy](docs/PRIVACY.md) and [Sources](docs/SOURCES.md).

## Contributing

You do not need to write Rust to contribute. Chess knowledge, source research, statistics, testing, documentation, data-quality review, historical work, UI integration, and reproducible bug cases are all useful.

For a small, obvious fix, opening a focused pull request directly is fine. For a new application, source, persisted format, identity rule, or broad architectural change, open an issue first so the boundary can be discussed cheaply.

Ideas are welcome even when they do not yet fit the roadmap. A proposal may be merged into Antiskill, incubated in **Laboratorio**, kept as an external integration, or declined after review; the important thing is to make the value and boundary testable.

See [Contributing](CONTRIBUTING.md) and [Contributor paths](docs/CONTRIBUTORI.md). Pull-request commits use the Developer Certificate of Origin 1.1 (`git commit -s`).

## Documentation

- [Presentation](docs/PRESENTAZIONE.md)
- [Project map](docs/MAPPA.md)
- [Shared base and verticals](docs/BASE_E_VERTICALI.md)
- [Verified and enriched corpus](docs/CORPUS_VERIFICATO.md)
- [Architecture](docs/ARCHITETTURA.md)
- [Possibilities](docs/POSSIBILITA.md)
- [Reports](docs/RAPPORTI.md)
- [Sources](docs/SOURCES.md)
- [Privacy](docs/PRIVACY.md)
- [Security](SECURITY.md)
- [Licensing](docs/LICENZE.md)
- [Roadmap](docs/ROADMAP.md)
- [Documentation index](docs/INDICE.md)

Deeper implementation documents are intentionally kept out of the main path unless they help a contributor make a concrete change.

## Status

Antiskill is under active development. The PGN archive workflow is the strongest public surface today. There is no required GUI, central service, or engine dependency; advanced source coverage, analysis, reports, and research remain open areas for careful growth.
