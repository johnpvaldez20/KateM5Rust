<!--
One packet, one branch, one PR. Fill every field. A PR that does not name
its Gate 1 PASS is not reviewed. See docs/governance/GOVERNANCE_LINUS.md.
-->

## Packet

- **Packet:** PKT-NNNN — Closes #NNNN
- **Gate 1 PASS:** <link to the issue comment holding the Gate 1 payload>
- **Branch base (`base_sha`):** <full 40-char SHA of main this branch started from>

## Provenance of the worker

```json
{
  "agent_id": "",
  "role": "CODER",
  "runtime": "LOCAL | CLOUD",
  "host": { "kind": "OWNER_MACBOOK | CURSOR_CLOUD", "name": "" },
  "model": { "provider": "", "name": "", "settings": {} },
  "tool": { "name": "CURSOR_IDE | CURSOR_CLOUD_AGENT | OLLAMA" }
}
```

## Scope

- [ ] Every modified file is in the Gate 1 `allowed_files` (Cargo.lock excepted when a dependency was added).
- [ ] No file under `upstream/`, `docs/governance/`, `.github/` was touched (unless this is a gatekeeper packet).
- [ ] No `unsafe`. No unrelated formatting. Every new dependency has a justification below.

## Dependencies added

| crate | version | why std or an existing dependency is not enough |
|---|---|---|
| — | — | — |

## Regression test

- **Name:** `crate::module::tests::name`
- **Red run (fails without the change):** <GitHub Actions URL on a commit without the change>
- **Green run (passes with it):** <GitHub Actions URL on this PR's head>

## Local runs (advisory, zero weight)

```
rustc --version   ->
command           ->
exit code         ->
```

<!-- Gate 2 and Gate 3 verdicts are posted by the gatekeeper as PR reviews
containing ```json gate-payload blocks. Do not post verdict payloads yourself. -->
