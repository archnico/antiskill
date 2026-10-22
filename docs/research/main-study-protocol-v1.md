# Antiskill Research v1 — multi-player main-study protocol

Status: **pre-registered protocol; no main-study outcomes inspected**  
Lineage: legacy development issue 76
Primary population: Lichess rapid, rated, human-vs-human games from calendar year 2025  
Reserved replication population: Lichess rapid, rated, human-vs-human games from 2026-01-01 through 2026-06-30  
Primary safe-move threshold: **80%**

Context across alternative Antiskill formulations: [research overview](QUADRO.md).

> This document freezes the main-study design before engine outcomes for the main dataset are inspected. Any change that could affect sample membership, opportunity classification, the primary endpoint, or the primary statistical test requires a new protocol version and must be made before inspecting main-study outcomes under that revised design.

## 1. Scientific question

Primary question:

> When a chess position offers many safe legal moves and only one or two catastrophic legal moves, do players below Lichess rapid rating 1200 choose catastrophic moves more frequently than a position-conditioned uniform-random legal-move baseline?

For decision `i`:

- `L_i` = number of legal moves;
- `K_i` = number of catastrophic legal moves;
- `p_i = K_i / L_i`;
- `Y_i = 1` if the human played a catastrophic move, otherwise `0`;
- `O = ΣY_i`;
- `E = Σp_i`;
- `O/E` = descriptive observed/expected ratio.

`O/E` is a descriptive effect summary, not by itself the confirmatory test statistic.

## 2. Epistemic scope

The main study is designed to estimate behavior in the declared Lichess rapid population and to test one pre-registered primary hypothesis.

It does not by itself establish:

- causality;
- FIDE Elo equivalence;
- behavior in over-the-board chess;
- behavior at other time controls;
- a universal psychological mechanism;
- a permanent Antiskill rating or player trait.

All references to rating in this study mean the focal player's **Lichess rapid rating recorded for that game**, not FIDE Elo.

## 3. Development, main, and replication separation

The completed exploratory pilot is development evidence only.

The study partitions evidence as follows:

1. **Development/pilot:** exploratory pilot v1 and all diagnostics already performed before this protocol.
2. **Main dataset:** eligible Lichess games dated `2025-01-01` through `2025-12-31` inclusive.
3. **Reserved replication dataset:** eligible Lichess games dated `2026-01-01` through `2026-06-30` inclusive.

The 2026 H1 replication set must not be used to tune thresholds, sampling, exclusion rules, engine parameters, model specification, or interpretation of the 2025 main result.

The replication protocol may be executed only after the main analysis code, main result, and any deviations from this protocol are frozen and recorded.

## 4. Required upstream source contract

Research must consume a frozen, provenance-bearing input view supplied by the archive/source layers rather than silently acquiring or rewriting external data inside the experiment.

For every candidate game the upstream input must provide enough verified information to determine at least:

- stable game/occurrence identity;
- game date;
- rated/unrated status;
- normalized speed/time-control class;
- White and Black account identity;
- White and Black rating recorded for the game;
- bot/human status sufficient to exclude bot games fail-closed;
- legal mainline moves or enough lossless game state to reconstruct them deterministically.

Main-study inclusion requires:

- date in calendar year 2025;
- rated game;
- normalized speed exactly `rapid`;
- both sides established as human, not bot;
- both account identities present;
- focal player's game rating present and parseable;
- standard chess, not a variant;
- mainline legal and reconstructable through the focal decision.

If the available source cannot establish one of these required predicates, the game is ineligible rather than guessed into the dataset.

Acquisition, source interpretation, archive construction, and identity reconciliation belong to their respective Antiskill modules and are out of scope for this Research issue.

## 5. Rating bands

The focal player's rating at the game determines exactly one band:

| Band | Lichess rapid rating |
|---|---:|
| B1 | `<1000` |
| B2 | `1000–1199` |
| B3 | `1200–1399` |
| B4 | `1400–1599` |
| B5 | `1600–1799` |
| B6 | `1800–1999` |
| B7 | `2000–2199` |
| B8 | `>=2200` |

The primary population is the union `B1 + B2`, i.e. focal rating `<1200`.

Bands are sampling/description strata. The primary confirmatory claim is not eight independent bandwise tests.

## 6. Sampling unit and anti-dominance rules

The sampled unit is a **human decision** by one deterministic focal player in one game.

To limit dependence and domination by prolific accounts:

- at most one focal player is selected per game;
- at most `5` sampled decisions may come from one game;
- at most `100` sampled decisions may come from one focal account across the complete main dataset;
- target sample is exactly `20,000` decisions per rating band;
- total target sample is therefore `160,000` decisions.

The caps are applied before engine analysis and without using whether the played move is safe, catastrophic, good, bad, or otherwise interesting.

Because one focal player is chosen per game, all sampled decisions from a game are nested within one focal account for inferential clustering.

## 7. Deterministic focal-player selection

A game is eligible for focal-player selection if at least one side satisfies all focal-player prerequisites, including a valid game rating.

If exactly one side is eligible, that side is focal.

If both sides are eligible, choose one side deterministically from a cryptographic hash over:

```text
main-study-v1 | FOCAL | <frozen-sampling-seed> | <stable-game-identity>
```

The least-significant bit of the digest chooses White or Black under a documented fixed convention.

The choice must not use moves, engine evaluations, result, or any post-game performance variable.

## 8. Candidate decision universe

For the selected focal player, candidate decisions are the focal player's legal decisions satisfying:

- `ply >= 16`;
- the game has not already terminated;
- the mainline through that decision is legal and reconstructable;
- the focal player's rating band is determined from the game's recorded rating.

`ply >= 16` is retained from the completed pilot because the pilot showed severe domination and weak construct validity in the earliest opening plies. It is not re-optimized on the main data.

No engine-derived property is used to decide which decisions enter the pre-analysis sampling universe.

## 9. Deterministic within-game decision cap

If a focal game supplies more than `5` eligible decisions, rank its decisions by a cryptographic hash over:

```text
main-study-v1 | DECISION | <frozen-sampling-seed> | <stable-game-identity> | <ply>
```

Retain the first `5` in ascending digest order.

If there are `5` or fewer, retain all.

## 10. Deterministic account cap and band sample

Within each rating band, the capped candidate decisions are assigned a deterministic sampling rank using a cryptographic hash over:

```text
main-study-v1 | SAMPLE | <frozen-sampling-seed> | <stable-game-identity> | <ply> | <focal-account-identity>
```

A concrete numeric sampling seed must be chosen and committed to the machine-readable frozen manifest **before any main-study engine analysis starts**. The seed must not be changed after outcome inspection.

Selection proceeds deterministically while enforcing the global account cap of `100` retained decisions per focal account. The first `20,000` admissible decisions in each rating band are retained.

If any band cannot reach `20,000` decisions from the frozen 2025 source under these rules, the main engine analysis must not start. The insufficiency must be documented and the design may be revised only in a new protocol version before inspecting main-study engine outcomes.

## 11. Fixed sample size and stopping rule

The study is fixed-size, not event-driven.

Target:

```text
8 bands × 20,000 decisions = 160,000 decisions
```

There is no stopping based on:

- number of catastrophic choices observed;
- `O/E`;
- p-values;
- confidence intervals;
- apparent effect direction;
- interim rating trends.

After the frozen sample is established, all selected decisions must be analysed or the run remains partial/failed.

## 12. Engine identity and search protocol

The main study inherits the deterministic engine protocol validated in the exploratory pilot.

Engine:

- Stockfish 18;
- binary SHA-256:
  `7cccf61665b0e67576634916ab476dcc9c567a395759725fd0990eb441ff80a5`.

Per worker:

- `Threads = 1`;
- `Hash = 64 MiB`.

For every sampled position:

- enumerate every legal move;
- analyse **every legal root move separately**;
- use exactly `50000` nodes per legal move;
- use `searchmoves` for that single root move;
- before every individual `searchmoves` call execute:
  1. `ucinewgame`;
  2. `Clear Hash`;
  3. `isready`;
- derive expected score from Stockfish WDL as `W + D/2` for the side to move.

No scan/prefilter/screening stage is allowed in the main analysis.

The number of parallel workers is operational rather than scientific, provided every worker uses the exact per-move protocol above and worker count is recorded in the run manifest.

## 13. Opportunity definition

For a position, let the best legal move have expected score `S_best` and move `m` have expected score `S_m`.

Define loss in percentage points:

```text
loss_pp(m) = 100 × (S_best - S_m)
```

A move is **safe** when:

```text
loss_pp <= 10
```

A move is **catastrophic** when:

```text
loss_pp >= 35
```

A position is an opportunity at threshold `T` only when all of the following hold:

- best move expected score `>= 35%`;
- catastrophic move count `K` is exactly `1` or `2`;
- `K/L <= 0.15`;
- safe-move fraction is at least `T`.

Thresholds frozen for reporting:

- `60%`;
- `70%`;
- **`80%` primary**;
- `90%`.

The thresholds are nested sensitivity views, not independent replications.

The pilot found `N60 = N70 = N80` in one player's corpus. That observation does not authorize changing the threshold grid after seeing the main result.

## 14. Primary endpoint

The primary endpoint uses only primary-threshold (`safe_ratio >= 0.80`) opportunities for focal players with game rating `<1200`.

For each opportunity:

```text
p_i = K_i / L_i
Y_i = 1 if the played move is catastrophic, else 0
```

Descriptive quantities include:

```text
O = ΣY_i
E = Σp_i
O/E
```

The primary inferential model is a logistic regression with the position-conditioned uniform baseline as a fixed offset:

```text
logit(P(Y_i = 1)) = logit(p_i) + β
```

Primary hypothesis:

```text
H0: β <= 0
H1: β > 0
```

Inference requirements:

- one-sided test at `alpha = 0.05` for the pre-registered direction `β > 0`;
- cluster-robust standard errors by focal account;
- report `β`, `exp(β)`, standard error, one-sided p-value, and a two-sided 95% confidence interval;
- also report `O`, `E`, and `O/E` descriptively.

`exp(β)` is an **odds multiplier** relative to the position-conditioned uniform baseline, not a risk ratio and not a causal effect.

If software/model failure prevents this exact pre-registered fit, the failure must be reported. An alternative model may be presented only as a clearly labelled deviation/sensitivity analysis, not silently substituted as the primary test.

## 15. Secondary analyses

Secondary pre-specified analyses do not replace the primary test.

### 15.1 Rating gradient

Across all primary-threshold opportunities, fit:

```text
logit(P(Y_i = 1))
  = logit(p_i)
  + β0
  + β1 × ((rating_i - 1200) / 100)
```

with cluster-robust standard errors by focal account.

Interpret `β1` as the change in log-odds multiplier per +100 Lichess rapid rating points, conditional on each position's `p_i` baseline.

The directional expectation `β1 < 0` is pre-specified, but this is a **secondary** analysis. It must not be described as an independently multiplicity-controlled confirmatory result.

### 15.2 Bandwise descriptions

For each of the eight rating bands, report at threshold 80%:

- sampled decisions;
- opportunities;
- opportunity rate;
- distinct focal accounts;
- distinct games;
- `O`;
- `E`;
- `O/E`;
- an uncertainty interval from a pre-specified account-cluster-aware method implemented before outcome inspection.

Bandwise p-values, if shown at all, are descriptive/sensitivity outputs and not eight primary tests.

### 15.3 Threshold sensitivity

Repeat descriptive summaries at 60%, 70%, and 90% safe thresholds.

These thresholds are nested and must not be treated as independent confirmations.

## 16. Dependence and clustering

Human decisions are not assumed independent merely because positions differ.

The primary inferential uncertainty must account for repeated decisions by the same focal account using account-clustered inference.

The design also limits dependence before analysis through:

- one focal player per game;
- maximum 5 sampled decisions per game;
- maximum 100 sampled decisions per focal account.

Nominal decision count must never be presented as though it were equivalent to the number of independent human subjects.

## 17. Manual construct-validity review

The main study includes a blinded manual chess review of the opportunity definition.

Before revealing the focal player's played move, construct a deterministic review set of `80` primary-threshold opportunities, stratified across rating bands and safe-ratio regions under a sampling rule committed in the analysis code/manifest before review.

For the blinded phase, the reviewer may see:

- FEN/board position;
- side to move;
- legal-move count;
- engine-classified catastrophic move(s);
- sufficient engine evidence/PV to understand the classification when needed.

The reviewer must **not** see which move the focal human actually played during the initial construct-validity judgment.

Record a categorical judgment such as:

- `valid catastrophe`;
- `debatable`;
- `anomaly/invalid`;

with a short reason.

Only after the blinded judgments are frozen may the human played move be revealed.

All actually observed catastrophic choices may then receive a second targeted chess audit for explanation and publication quality. That post-outcome audit:

- is quality control;
- must not change the frozen catastrophe definition;
- must not delete inconvenient observations from the primary analysis;
- must not retroactively alter sample membership.

Any genuine data corruption or legality failure is handled under the pre-specified data-integrity rules, not subjective outcome-based exclusion.

Public case studies are anonymized by default. Account names, game IDs, and other individual identifiers are disclosed only when they are scientifically necessary and explicitly justified; aggregate results do not require them.

## 18. Data-integrity exclusions

A sampled decision may be excluded only for a pre-existing objective integrity failure such as:

- source bytes unavailable or failing provenance verification;
- game/mainline corruption;
- illegal or unreconstructable game state;
- missing required focal identity/rating after the frozen sample is checked;
- engine process failure that cannot be resolved by clean retry under the identical protocol;
- mismatch between canonical sample identity and analysed position.

Every exclusion must be logged with a machine-readable reason.

No exclusion may depend on:

- whether the move was catastrophic;
- whether it favors or disfavors the hypothesis;
- the resulting `O/E`;
- whether a position looks aesthetically strange after the result is known.

If an integrity exclusion occurs after sample freeze, replacement is permitted only by continuing the original deterministic band sampling order to the next admissible decision under the same caps. Replacement identity and reason must be logged.

## 19. Long-run execution and canonical evidence

The successful exploratory pilot exposed a runner-resilience failure mode. The main runner must therefore be resumable and fail-closed.

Required run properties:

- frozen sample manifest written before analysis;
- append-only canonical journal of completed positions;
- no partially analysed position enters the canonical journal;
- deterministic resume by stable decision identity;
- engine death causes clean engine restart and retry of the same position;
- bounded retry count with explicit failed/partial status if exhausted;
- errors/restarts in append-only logs;
- final manifest records complete/partial/failed state;
- canonical output can be deterministically sorted before final hashing;
- runner source hash and analysis code version/commit recorded;
- engine binary hash recorded;
- input view identity/hash recorded;
- frozen sampling seed recorded;
- final canonical journal hash recorded.

Operational paths, hostnames, and private corpora are not public protocol identities.

## 20. Per-move evidence retention

Unlike the first pilot runner, the main study must preserve enough per-legal-move engine evidence to audit threshold margins without rerunning the entire study.

At minimum for each analysed legal move retain or make reproducibly derivable from a canonical sidecar/artifact:

- move UCI;
- WDL returned by the engine;
- expected score;
- node budget/profile identity.

The storage format is not frozen here; it must remain provenance-bearing, deterministic enough for audit, and not be silently discarded after deriving position-level classifications.

## 21. No interim outcome inspection

Before the main dataset is complete and frozen, progress monitoring may inspect only operational information such as:

- completed count;
- failures;
- restart count;
- throughput;
- ETA;
- disk/memory health.

Do not inspect interim:

- candidate/opportunity counts by rating;
- `O`;
- `E`;
- `O/E`;
- catastrophic-choice identities;
- regression coefficients;
- p-values;
- effect direction.

If an operational defect requires changing the scientific protocol, stop the run, document the defect, and create a new protocol version before inspecting outcomes from a revised run.

## 22. Sample-size rationale

The fixed target is `20,000` sampled decisions per band (`160,000` total).

The exploratory pilot produced an 80%-threshold opportunity rate of about `2.7%` and average uniform catastrophe probability among opportunities of roughly `0.044`.

Those pilot values are used only for scale planning. If similar magnitudes held in the main study, the `<1200` primary sample of `40,000` decisions would yield on the order of one thousand opportunities and roughly several dozen catastrophic choices expected under the uniform baseline.

This is not a guarantee and not an event-based stopping rule. The actual study remains fixed at the pre-analysis decision counts above.

## 23. Required reporting

The main report must distinguish visibly:

1. **Observed data** — counts, opportunity rates, `O`, `E`, `O/E`.
2. **Statistical inference** — model coefficient, uncertainty, p-value, and model assumptions.
3. **Chess interpretation** — what the manually reviewed catastrophic moves appear to represent over the board.
4. **Generalization** — exactly which Lichess population/time control/time period the data support.

Do not transfer certainty between these layers.

In particular:

- a statistically significant result does not prove a psychological mechanism;
- a null result does not prove the effect is absent;
- a large `O/E` without uncertainty is not sufficient evidence;
- Lichess rating must not be relabelled FIDE Elo;
- the 2025 main result is not automatically a replication of the single-player pilot.

## 24. Replication boundary

The reserved 2026 H1 dataset is a genuine temporal replication target.

After the 2025 main analysis is frozen:

- execute the same eligibility rules;
- use the same rating bands;
- use the same `ply >= 16` rule;
- use the same sample-size targets and caps unless impossibility was already documented without examining replication outcomes;
- use the same engine identity and per-move protocol;
- use the same opportunity definition;
- use the same primary threshold and primary model;
- use a separately frozen replication sampling seed.

Any change made after seeing the 2025 main result must be identified as a changed-protocol replication and cannot be described as an exact replication.

## 25. Frozen versus deferred details

Frozen by this protocol:

- target population/time windows;
- rating bands;
- fixed decision counts;
- player/game caps;
- one-focal-player-per-game rule;
- `ply >= 16`;
- all-moves exact engine analysis;
- Stockfish binary identity;
- 50k nodes/move;
- reset-before-every-searchmove;
- safe/catastrophe definitions;
- best-score/K/K-L constraints;
- 80% primary threshold;
- primary `<1200` population;
- primary offset-logistic model;
- account clustering;
- blinded manual review principle;
- no-interim-outcome rule;
- 2026 H1 replication holdout.

Deferred but required before engine analysis starts:

- concrete numeric main sampling seed;
- exact frozen upstream input-view identity/hash;
- exact implementation commit/hash for sample construction and analysis;
- exact machine-readable schema for canonical sample/run artifacts;
- exact account-cluster-aware interval implementation used for bandwise descriptive estimates;
- exact deterministic allocation rule for the 80-position blinded manual-review sample;
- operational worker count and storage locations.

These deferred details may be frozen in an execution-manifest issue without changing the scientific design. None may be selected using main-study outcome information.

## 26. Out of scope

This protocol issue does not:

- acquire the Lichess corpus;
- implement Source/Archive ingestion;
- change parser or Dedup semantics;
- redesign persistent archive formats;
- run the 2025 main experiment;
- inspect main-study outcomes;
- run the 2026 H1 replication;
- create product/GUI features;
- claim causal mechanisms;
- map Lichess rating to FIDE Elo;
- define a permanent Antiskill score.

## 27. Next gate

Before any 2025 main-study Stockfish analysis begins, a separate execution-freeze step must record and verify:

1. the exact frozen 2025 input view and provenance hash;
2. the concrete sampling seed;
3. the exact 160,000-decision sample identity/hash;
4. implementation/runner hash or commit;
5. canonical run/output schemas;
6. blind-review sample allocation algorithm;
7. statistical implementation/version and synthetic validation tests;
8. disk/runtime feasibility for the full all-legal-moves 50k protocol.

Only after that gate is merged may the main run begin.
