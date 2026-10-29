# Governance package

| File | What it is |
|---|---|
| `GOVERNANCE_LINUS.md` | The contract. Laws, roles, the three gates, lanes, stale-branch and conflict handling, branch protection. Read it before you touch anything. |
| `schemas/common.schema.json` | Shared definitions: provenance, runtime, host, model, GitHub references, lane, protected paths. |
| `schemas/gate1-planning-handoff.schema.json` | Gate 1 verdict payload. |
| `schemas/gate2-coding-handoff.schema.json` | Gate 2 verdict payload. |
| `schemas/gate3-qa-merge-handoff.schema.json` | Gate 3 verdict payload. |
| `examples/valid/` | Payloads that must validate, including the PKT-0142 test-sabotage rejection. |
| `examples/invalid/` | Payloads that must be rejected, one defect each. |

All schemas are JSON Schema 2020-12. `cargo test --workspace` runs
`tools/gatecheck` over schemas, examples and `docs/design/packets/`.

Previous version (v1, service-oriented: Postgres namespaces, jest, cache
keys) is superseded. Differences that matter: the hybrid local/cloud model,
GitHub as the only handoff channel, provenance on every payload, CI-only
evidence at Gate 3, protected paths, full SHAs, structured blocking defects,
and the enum typo `SPECULATIVE_GENERality` fixed to `SPECULATIVE_GENERALITY`.
