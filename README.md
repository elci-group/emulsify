# Outcome: Identify how each fork should be handled (mix, emulsify, suspend, reject)

## Outcome: Immediate, actionable compatibility analysis for Rust engineers

**emulsify** provides a viscosity‑based compatibility analysis and a nearest‑neighbour
consolidation plan. It does **not** perform merges or read git history.

---

## Outcome: Understand the analysis flow – Pipeline Overview

```text
measure → classify → find neighbours → consolidate → validate → bind
```

The pipeline yields the data needed to decide whether a fork can be mixed
directly, needs an adapter, stays independent, or must be rejected.

---

## Outcome: Classify each fork – The Four Consolidation States

| state      | movement                     | meaning                                             |
|------------|------------------------------|-----------------------------------------------------|
| `mix`      | `fork → host`                | low viscosity; directly incorporable                |
| `emulsify` | `fork → adapter → host`      | compatible, but requires an intermediate adapter   |
| `suspend`  | `fork ⇢ host`                | valuable but divergent; keep independent behind an interface |
| `reject`   | `fork ⛔ host`                | effectively immiscible; do not merge directly       |

---

## Outcome: Score forks on nine dimensions – Compatibility Evaluation

Each fork receives a score in **[0, 1]** against the host on the following dimensions:

- **api** — named symbol overlap (functions, types, modules)
- **types** — struct/enum/class/interface name overlap
- **dependencies** — declared dependency overlap (Cargo.toml, etc.)
- **architecture** — directory‑skeleton overlap (depth ≤ 2)
- **behaviour** — function‑name overlap (the behavioural surface)
- **tests** — test‑file overlap
- **ownership** — domain overlap below conventional roots like `src/`
- **lineage** — identical files at identical paths across the union of file sets
- **abstraction** — trait/interface name overlap

Dimensions absent on both sides are reported as `n/a` and omitted from the weighted
sum rather than scored zero. Weights are configurable and normalised automatically.

---

## Outcome: Produce a bind order – Nearest‑Neighbour Consolidation Planning

*Nearest‑neighbour* refers to the most compatible architectural state, not the
nearest commit.

1. Repeatedly bind the globally most‑compatible **unbound fork** to its most
   compatible member (which may be another consolidated fork).
2. Classify any fork that cannot bind: oily forks **suspend** behind an
   interface boundary; immiscible forks are **reject**ed.
3. Group suspended forks that mix well with each other and **consolidate** them
   together, separate from the host.

---

## Outcome: Install the tool locally – Installation

```sh
cargo install --path .
```

---

## Outcome: Generate the information you need – Usage

```sh
# Measure every fork's viscosity against the host
emulsify analyze ./host ./fork-a ./fork-b ./fork-c

# Produce an ordered consolidation plan (most compatible first)
emulsify plan ./host ./fork-a ./fork-b ./fork-c

# Show the full pairwise compatibility matrix
emulsify graph ./host ./fork-a ./fork-b

# Machine‑readable JSON output for any command
emulsify --json plan ./host ./fork-a > plan.json
```

### Example plan output

```text
Consolidation plan for host — nearest-neighbour bind order

 1. mix      fork-a -> host                            compat 0.94  direct bind
 2. mix      fork-c -> host via fork-a                 compat 0.95  direct bind
 3. emulsify fork-b -> host                            compat 0.61  consolidation adapter
 4. suspend  fork-d <=> host boundary (nearest: fork-b) compat 0.44  keep independent, establish interface
 5. reject   fork-e                                   compat 0.22  immiscible — do not merge directly

suspension groups (consolidate together, apart from the host):
  [fork-f, fork-g]
```

---

## Outcome: Tune thresholds and weights – Configuration

`emulsify init` writes an example `emulsify.toml`. The binary loads it from the
working directory or via `--config`.

```toml
[weights]
api = 0.15
types = 0.10
dependencies = 0.15
architecture = 0.15
behaviour = 0.10
tests = 0.10
ownership = 0.05
lineage = 0.10
abstraction = 0.10

[thresholds]
mix = 0.75
emulsify = 0.50
suspend = 0.30
```

Only listed weights override defaults; the total is normalised automatically.

---

## Outcome: Know what emulsify does **not** do – Honest Limitations

- Extraction uses deterministic line‑scanning, not full parsing; symbol sets are
  heuristic surfaces, not semantic models.
- Behaviour, ownership and lineage are *proxies* (function‑name overlap,
  domain‑directory overlap, identical‑file overlap). emulsify compares
  architectural state; it does **not** execute code or read git history.
- emulsify produces a **plan**, not a merge. You must apply the integration
  pressure yourself.
- emulsify is deliberately narrow: deterministic/semi‑deterministic
  consolidation based on compatibility, viscosity and nearest‑neighbour
  relationships. Higher‑level synthesis (e.g., Hybreed) is a separate concern.

---

## Outcome: Verify the tool works as expected – Development

```sh
cargo test --lib --tests   # unit + integration suites
cargo build --release
```

---

## Outcome: Legal terms – License

MIT
