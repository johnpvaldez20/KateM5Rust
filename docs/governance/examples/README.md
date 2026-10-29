# Gate payload examples

`valid/` must validate. `invalid/` must be rejected. `cargo test --workspace`
runs `gatecheck` over both, plus every packet in `docs/design/packets/`.

The invalid files are the valid ones with exactly one thing wrong. The file
name says what. If you change a schema and one of these starts validating, you
have opened a hole; if a valid one stops validating, you have broken a
contract. Either way the test tells you before the gatekeeper does.

| invalid file | the one thing wrong | rule it trips |
|---|---|---|
| `gate1-pass-without-definitions` | PASS with no type definitions | data contracts first |
| `gate1-pass-with-flagged-anti-pattern` | PASS with `SPECULATIVE_GENERALITY` flagged | no speculative abstraction |
| `gate1-coder-touches-protected-path` | a CODER packet lists `.github/workflows/ci.yml` | protected paths are the gatekeeper's |
| `gate1-coder-scope-under-upstream` | a CODER packet lists a file under `upstream/` | same rule; upstream is read-only |
| `gate1-cloud-lane-needs-metal-gpu` | lane CLOUD but requires `METAL_GPU` | hybrid lane rules |
| `gate2-pass-with-out-of-scope-files` | PASS with a file outside Gate 1 scope | subsystem isolation |
| `gate2-ollama-model-claims-cloud-runtime` | Ollama model with runtime CLOUD | provenance consistency |
| `gate2-pass-on-stale-base` | PASS while `merge_base_is_main_head` is false | stale branches rebase first |
| `gate2-pass-with-unsafe-block` | PASS with one `unsafe` block | workspace forbids unsafe |
| `gate2-dependency-without-justification` | justification is the word "convenient" | no unjustified dependencies |
| `gate3-merge-approved-with-failed-ci` | approved with CI FAILURE and failed tests | trust execution |
| `gate3-merge-approved-without-ci-run` | approved with CI MISSING | only CI counts |
| `gate3-abbreviated-sha` | 7-character SHA | full SHAs only |
| `gate3-ci-run-for-different-commit` | CI head differs from PR head (caught by gatecheck code, not schema) | evidence is per commit |
| `gate3-merge-approved-with-ignored-tests` | `#[ignore]` added | no test sabotage |
| `gate3-merge-approved-regression-test-not-red` | regression test never failed | red then green |
| `gate3-handoff-outside-github` | payload posted to Slack | GitHub is the only channel |
| `gate3-merge-approved-with-network-tests` | tests opened sockets | isolation |
| `gate3-merge-approved-coverage-below-floor-no-waiver` | 61% coverage, no waiver | coverage floor |
| `gate3-unknown-top-level-field` | extra `notes` field | no hallucinated fields |
| `gate3-merged-by-coder` | merge performed by a CODER | only the gatekeeper merges |
