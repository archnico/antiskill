# Antiskill project rules

## Product boundary

Antiskill ships software, not a central database. Users build local archives from data they are entitled to use.

Guiding formula:

> Italian specialization, international architecture.

## Areas

Use the short Italian vocabulary consistently:

**Nucleo, Ricerca, Rapporti, Analisi, Fonti, Interoperabilità, Conoscenza, Piattaforma, Laboratorio, Qualità.**

These are ownership boundaries, not permanent Git branches. See `docs/MAPPA.md`.

The complementary product model is **shared base + vertical slices** (`docs/BASE_E_VERTICALI.md`). The base is not a new monolithic area: it is the set of stable capabilities reused across verticals. A vertical may cross several areas but must not create shadow semantics for ingest, identity, deduplication, provenance, query, or job execution merely to ship one report faster. Promote shared capability into the base only with a clear contract and tests.

In particular:

- Nucleo must not require network access, GUI, engine, external orchestration, private corpora, or machine-specific configuration;
- Ricerca consumes frozen inputs and must not redefine Nucleo identities to fit an outcome;
- Analisi is optional and reusable;
- Fonti own acquisition and source-specific network behaviour;
- Piattaforma may schedule work but must not change its semantics;
- Laboratorio cannot silently become a stable dependency.

## Data principles

- Do not silently delete or overwrite source evidence.
- Derived data must remain traceable to its sources.
- Identity, merge, and deduplication decisions must be explicit and reversible.
- Lossless PGN paths preserve original occurrence bytes.
- Parsers fail clearly on unsupported or inconsistent input instead of inventing missing facts.
- Persisted chains are verified fail-closed before use.
- Source rights and redistribution constraints are part of source design.
- Direct dependencies are also a distribution decision: review licence and artifact obligations before adding one.

## Privacy

Public material follows `docs/PRIVACY.md`.

Prefer synthetic fixtures. Do not commit credentials, private corpora, unnecessary personal identifiers, local machine details, or third-party material with unclear redistribution rights.

Research involving real people or accounts is anonymized by default unless identity is necessary and explicitly justified.

## Scope

Implement the smallest coherent change that proves value.

Small fixes may go directly to a focused PR. Discuss persisted formats, identity rules, network behaviour, source-rights assumptions, or broad architecture before implementation.

Useful applications may live in the main repository, Laboratorio, an adapter, an interoperability package, or an external companion project. Do not force every good idea into Nucleo.

## Contributions

Do not push directly to `master`.

Pull-request commits use DCO 1.1 and must include a `Signed-off-by` line, normally via:

```bash
git commit -s
```

Contributors retain copyright in their work; contributions are licensed under EUPL-1.2.

## Tests

Prefer deterministic, minimal, synthetic fixtures.

Normal Rust changes should pass:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --locked
```

CI covers Linux, Windows, and macOS. Manual real-corpus pilots remain optional and cannot be required by the normal suite.

## Documentation

Keep these pairs equivalent in substance:

- `README.md` ↔ `LEGGIMI.md`;
- `CONTRIBUTING.md` ↔ `CONTRIBUIRE.md`.

Italian is the working language for fast-moving documentation. English polish is useful but is not a publication or merge gate.

Keep the public path short. Historical and acceptance documents may remain available but should not dominate the introduction.

## Maturity

The local PGN workflow (`init`, `ingest`, `doctor`, `compare`, `search`, `duplicates`, `export`) is the most mature product surface.

Documentation must distinguish implemented support, experimental work, source registries, and future possibilities.
