# Contributing to Antiskill

[Italiano](CONTRIBUIRE.md) · [README](README.md)

Antiskill welcomes code, domain knowledge, source research, statistics, testing, documentation, historical work, and integration ideas. **You do not need to know the whole project or write Rust.**

## Start with the smallest useful contribution

- **Small fix:** open a focused pull request directly.
- **Bug:** open a bug report or a pull request with a minimal reproducer.
- **New application or idea:** use the General contribution template; a prototype is welcome but not required.
- **New source or adapter:** use the Source template.
- **New report:** use the Report template.
- **Persisted format, identity rule, network behaviour, or broad architecture:** discuss it in an issue before implementation.

Blank issues remain enabled. If you are unsure where something belongs, say so; maintainers can classify it later.

## Areas

| Area | Typical contributions |
|---|---|
| **Nucleo** | PGN, storage, identity, deduplication, indexing, search, export, performance |
| **Ricerca** | protocols, statistics, replication, human-move models |
| **Rapporti** | player, repertoire, tournament, corpus, source reports |
| **Analisi** | Stockfish, Syzygy, UCI, reusable analysis storage |
| **Fonti** | federations, tournament systems, broadcasts, archives |
| **Interoperabilità** | CLI/formats, En Croissant, Scid, scripts, APIs |
| **Conoscenza** | openings, events, historical and reference data |
| **Piattaforma** | jobs, cache, checkpoint, resume, portable execution |
| **Laboratorio** | bounded experiments and prototypes |
| **Qualità** | tests, fixtures, portability, documentation, privacy |

See [Contributor paths](docs/CONTRIBUTORI.md), the [Project map](docs/MAPPA.md), and [Shared base and verticals](docs/BASE_E_VERTICALI.md).

The areas above assign **ownership**. Product work uses the complementary **shared base + vertical slices** model: if a capability is useful to several applications, put its stable contract in the shared base under the area that owns its semantics; if it is specific to one use, keep it in that vertical and reuse the base instead of rebuilding ingest, identity, deduplication, provenance, query, or job machinery.

## Applications are intentionally open

A contribution does not need to be on a pre-approved feature list. We are interested in useful applications of a trustworthy chess archive, including ideas we have not anticipated.

After a proposal is tested, it may become:

- part of the Nucleo;
- a Source adapter;
- a Report or Analysis recipe;
- an interoperability package;
- a Laboratorio prototype;
- an independent companion project;
- or something we decide not to merge.

Merge decisions consider value, evidence, maintenance cost, privacy, rights, architectural fit, and whether the same goal is cleaner outside the core.

## Pull requests

Keep one pull request focused on one concern. An issue is recommended for substantial work, but it is not bureaucracy for its own sake: small obvious fixes do not need to wait for assignment.

A good PR explains:

- what changes and why;
- how it was verified;
- any data, privacy, rights, compatibility, or persisted-format impact;
- whether the change adds shared-base capability or vertical-specific logic, and which verticals benefit or which base contracts are reused;
- what is deliberately left out, when that matters.

Do not push directly to `master`.

## DCO

Antiskill uses the **Developer Certificate of Origin 1.1**. Sign each commit with:

```bash
git commit -s
```

If you forgot on the latest commit:

```bash
git commit --amend -s --no-edit
```

The sign-off certifies that you have the right to submit the contribution under the project license. Contributors retain copyright in their contributions.

## Local checks

For Rust changes, the normal pre-PR checks are:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --locked
```

Documentation-only changes do not need to run the full Rust suite locally; CI will still run the repository checks.

## Tests and data

Prefer deterministic, minimal, synthetic fixtures. Do not upload private corpora, credentials, unnecessary personal data, machine-specific details, or bulk third-party material with unclear redistribution rights.

When a real case is necessary, reduce it to the smallest redistributable example or keep the raw evidence outside the public repository and document only what is needed to reproduce the behaviour safely.

Source parsers should fail clearly on unsupported or inconsistent input rather than invent missing facts. Persistence and identity changes must preserve explicit provenance and fail-closed verification.

See [Privacy](docs/PRIVACY.md), [Invariants](docs/INVARIANTS.md), and [Sources](docs/SOURCES.md).

## Documentation

`README.md` and `LEGGIMI.md` should remain equivalent in substance, as should `CONTRIBUTING.md` and `CONTRIBUIRE.md`. Italian is the working language for fast-moving project documentation; English entry points are maintained on a best-effort basis and imperfect English is **not** a publication or merge blocker. Code-adjacent documentation may use one language when duplication would add more maintenance than clarity.

Use the short area names consistently: **Nucleo, Ricerca, Rapporti, Analisi, Fonti, Interoperabilità, Conoscenza, Piattaforma, Laboratorio, Qualità**.

## Review

CI passing is necessary, not sufficient. Maintainers may ask for a smaller boundary, stronger evidence, synthetic tests, clearer rights, or a different integration point.

Review is also allowed to conclude that an idea is valuable but belongs outside the main repository. That is not a failed contribution: keeping the Nucleo small is part of the design.

## License

Antiskill software is licensed under **EUPL-1.2**. Software licensing does not automatically relicense third-party chess data.
