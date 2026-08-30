# emulsify Documentation

**Outcome:** Decide whether divergent forks can be consolidated and obtain a concrete
plan that respects the viscosity metaphor.

---

## Decision Guide

**Decision:** Run `emulsify analyze` or `emulsify plan` to classify each fork against
the host into one of the four consolidation states.

| State | Compatibility threshold | Action |
|-------|--------------------------|--------|
| **mix** | ≥ 0.75 | Fork is **watery**; bind it directly into the host. |
| **emulsify** | 0.50 – 0.74 | Fork is **medium**; consolidate through an adapter. |
| **suspend** | 0.30 – 0.49 | Fork is **oily**; keep it independent behind an interface boundary. |
| **reject** | < 0.30 | Fork is **immiscible**; do not attempt consolidation. |

Thresholds are configurable via `emulsify.toml`; the defaults are `mix 0.75`,
`emulsify 0.50`, `suspend 0.30`.

---

## Compatibility Dimensions

**Decision:** Evaluate all nine dimensions before choosing a state.

1. **api** — named symbol overlap (functions, types, modules).
2. **types** — struct/enum/class/interface name overlap.
3. **dependencies** — declared dependency overlap (Cargo.toml, etc.).
4. **architecture** — directory-skeleton overlap (depth ≤ 2).
5. **behaviour** — function-name overlap (the behavioural surface).
6. **tests** — test-file overlap.
7. **ownership** — domain overlap below conventional roots like `src/`.
8. **lineage** — identical files at identical paths across the union of file sets.
9. **abstraction** — trait/interface name overlap.

A dimension absent on both sides is reported as `n/a` and excluded from the weighted
sum rather than scored zero. Weights are configurable and normalised automatically.

---

## Usage Workflow

**Outcome:** Produce a reproducible compatibility report and consolidation plan.

```text
measure → classify → find neighbours → consolidate → validate → bind
```

1. **Measure** — `emulsify analyze <host> <fork>...` scores every fork against the
   host across the nine dimensions and prints a per-dimension breakdown.
2. **Classify** — each fork's weighted score maps to one of the four states.
3. **Plan** — `emulsify plan <host> <fork>...` orders forks into a nearest-neighbour
   bind sequence, grouping forks that should be **suspend**ed together.
4. **Inspect** — `emulsify graph <host> <fork>...` prints the full pairwise
   compatibility matrix, including compatibility between forks themselves.
5. Add `--json` to any command for machine-readable output.

---

## Command Reference

| Command | Description |
|---------|-------------|
| `cargo build --release` | Build the release binary (expected exit 0). |
| `./target/release/emulsify --version` | Show version (`emulsify 0.1.0`). |
| `./target/release/emulsify analyze <host> <fork>...` | Score forks against the host. |
| `./target/release/emulsify plan <host> <fork>...` | Emit a nearest-neighbour consolidation plan. |
| `./target/release/emulsify graph <host> <fork>...` | Print the pairwise compatibility matrix. |
| `./target/release/emulsify init` | Write an example `emulsify.toml`. |

---

## Example Session

```bash
# Build the tool
cargo build --release

# Verify the binary
./target/release/emulsify --version
# -> emulsify 0.1.0

# Analyze forks against a host
./target/release/emulsify analyze ./host ./fork-a ./fork-b

# Produce a consolidation plan, machine-readable
./target/release/emulsify --json plan ./host ./fork-a ./fork-b > plan.json
```

---

## Honest Limitations

- **emulsify** only *plans* consolidation; it does **not** execute merges or read
  git history.
- Extraction uses deterministic line-scanning, not full parsing; symbol sets are
  heuristic surfaces, not semantic models.
- Behaviour, ownership and lineage are *proxies*, not measurements of runtime
  behaviour.

---

## Policy Reminder

- Never commit secrets, API keys, or credentials. Store them in environment
  variables or a secret manager.
- Run the full test suite before any change is considered complete:

  ```bash
  cargo test --lib
  cargo test --test integration_test
  ```

- Keep documentation in sync with code changes; update this file whenever
  behaviour changes.

---

*This file is managed by poka. Edits will be overwritten on the next `poka apply`
unless the corresponding rule is disabled in `poka.toml`.*
