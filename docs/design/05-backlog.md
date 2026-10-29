# Backlog: sequenced work packets

Every packet is one Gate 1 payload, one issue, one branch, one squash
commit. Phase 0 and Phase 1 packets exist as JSON under `packets/` and
validate against `docs/governance/schemas/gate1-planning-handoff.schema.json`
(run `cargo run -p gatecheck`). Later phases are listed here as titles with
lane and dependencies; each becomes a JSON payload when the gatekeeper opens
its issue, not before, because writing a Gate 1 payload for a packet eight
dependencies away is speculation with extra steps.

## Lane legend

- **CLOUD** — Cursor cloud agent, Linux VM, frontier model. Pure Rust crates,
  heavy tests, no macOS dependency. Default for library work.
- **LOCAL** — owner's MacBook Pro: Cursor IDE agent or an Ollama-served
  open-weight model. Anything needing macOS, Metal, a display, APFS, or the
  owner's eyes. Also small mechanical packets that fit a 64k context and are
  cheap to run locally.
- **EITHER** — fully specified, small, with a file-based oracle; good
  calibration packets for a new local model.

Parallelism comes from disjoint crates. At any moment the intended picture
is: one cloud lane in `kate-text` → `kate-core`, one cloud lane in
`kate-syntax`, one cloud lane in `kate-search`/`kate-fileio`, one local lane
for anything macOS-shaped. Four lanes, four crates, zero merge conflicts by
construction (`gatecheck` enforces scope disjointness).

## Phase 0 — Governance and skeleton (this PR)

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0001 | Bootstrap: governance, schemas, gatecheck, workspace, CI, design plan | LOCAL (owner's credentials, owner's review) | — |

## Phase 1 — Core model, no window

| Packet | Title | Lane | Depends on | Why this lane |
|---|---|---|---|---|
| PKT-0002 | `kate-text`: TextBuffer, Offset, CharRange, Edit, Transaction | CLOUD | 0001 | Pure rope + property tests. |
| PKT-0003 | `kate-text`: EditLog, Anchor, revision transform | CLOUD | 0002 | Same crate, same lane, serial. |
| PKT-0004 | `kate-text`: History (undo/redo, coalescing, saved depth) | CLOUD | 0003 | Same. |
| PKT-0005 | `kate-fileio`: decode and load | CLOUD | 0001 | Byte-level, platform-independent; depends on `ropey` only. |
| PKT-0006 | `kate-fileio`: encode and atomic save | **LOCAL** | 0005 | APFS rename/mode semantics; CI on macOS confirms. |
| PKT-0007 | `kate-syntax`: definition model + XML loader + 6 MIT definitions | CLOUD | 0001 | Independent crate; parallel lane. |
| PKT-0008 | `kate-syntax`: engine + upstream golden tests | CLOUD (frontier model) | 0007 | Hardest logic in Phase 1; best oracle. |
| PKT-0009 | `kate-syntax`: themes | EITHER (good for a local open-weight model) | 0008 | Two JSON files and a 30-entry enum. |
| PKT-0010 | `kate-search`: plain/regex search, replace transaction | CLOUD | 0002 | Parallel lane. |
| PKT-0011 | `kate-core`: Selections, movement, fan-out | CLOUD | 0003 | Pure logic. |
| PKT-0012 | `kate-core`: Document, View, Workspace, SyntaxCache, Command | CLOUD (frontier model) | 0004, 0005, 0008, 0011 | First integration; large context. |
| PKT-0013 | `kate-ui`: GPUI spike, read-only render, scroll benchmark | **LOCAL** (Metal, display, owner) | 0009, 0012 | The toolkit decision gate. |

Critical path: 0002 → 0003 → 0004 → 0012 → 0013. The syntax lane
(0007 → 0008 → 0009) and the search/fileio lane (0005 → 0006, 0010) run
alongside and finish before 0012 needs them.

## Phase 2 — An editor you can use

Titles only; JSON payloads are written when each issue opens.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0014 | `kate-ui`: editing input → `Command`, caret and selection painting, blink, scroll-to-cursor | LOCAL | 0013 |
| PKT-0015 | `kate-ui`: file open/save panels, Cmd-S/Cmd-O/Cmd-N/Cmd-W, modified indicator, unsaved-changes sheet | LOCAL | 0014 |
| PKT-0016 | `kate-core`: `EditorConfig` TOML (`~/Library/Application Support/KateM5Rust/config.toml`), `DocumentConfig` defaults | CLOUD | 0012 |
| PKT-0017 | `kate-core`: modelines (`kate:` variables) and `.editorconfig` | CLOUD | 0016 |
| PKT-0018 | `kate-core`: indentation — `normal` and `cstyle` indenters, auto-indent on newline, Tab/Shift-Tab | CLOUD | 0016 |
| PKT-0019 | `kate-core`: edit rules — auto-brackets, smart home, remove trailing spaces on save, trailing newline | CLOUD | 0018 |
| PKT-0020 | `kate-core`: folding — `FoldRegion` from syntax `FoldEvent`s and indentation; fold/unfold commands | CLOUD | 0012 |
| PKT-0021 | `kate-ui`: dynamic word wrap layout, visual-line Up/Down via `Command::MoveTo` | LOCAL | 0014 |
| PKT-0022 | `kate-ui`: icon border — line numbers, marks, fold markers, modified-line bar | LOCAL | 0020 |
| PKT-0023 | `kate-ui`: find/replace bar on `kate-search` (incremental, highlight all, regex toggle) | LOCAL | 0010, 0014 |
| PKT-0024 | `kate-core` + `kate-ui`: multi-cursor UI — Alt-click, Cmd-D select-next, Alt-Shift-Up/Down (two packets if the diff is big) | LOCAL | 0014 |
| PKT-0025 | `kate-core`: `SplitTree` operations and tab model; `kate-ui`: tabs and splits | LOCAL | 0015 |
| PKT-0026 | `kate-session`: `Session` JSON, autosave on quit, restore on launch | CLOUD | 0025 |
| PKT-0027 | `kate-ui`: file watcher (`notify`/FSEvents) → `Command::CheckDisk` → reload/keep sheet | LOCAL | 0015 |
| PKT-0028 | `kate-syntax`: vendor the remaining MIT definitions + upstream golden tests for each (may be split alphabetically) | EITHER | 0008, owner's license decision for non-MIT files |
| PKT-0029 | `kate-ui`: theme switching, system light/dark following | LOCAL | 0009 |

## Phase 3 — Workspace features

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0030 | `kate-session`: `.kateproject` reader, `Project` file list (git / directory walk) | CLOUD | 0026 |
| PKT-0031 | `kate-ui`: project tree pane, document list pane | LOCAL | 0030 |
| PKT-0032 | `kate-core`: fuzzy matcher (port of `kfts_fuzzy_match.h`, MIT); `kate-ui`: quick open / command palette | LOCAL | 0031 |
| PKT-0033 | `kate-lsp`: JSON-RPC over stdio, lifecycle, `ServerConfig` from Kate-compatible `settings.json`, position encoding | CLOUD | 0012 |
| PKT-0034 | `kate-lsp`: `didOpen`/`didChange` from transactions, diagnostics with anchors | CLOUD | 0033 |
| PKT-0035 | `kate-ui`: diagnostics underline + pane, hover | LOCAL | 0034 |
| PKT-0036 | `kate-core`: completion model (word completion); `kate-ui`: completion popup | LOCAL | 0014 |
| PKT-0037 | `kate-lsp`: completion, go to definition, references | CLOUD | 0034 |
| PKT-0038 | `kate-core`: recovery journal (`HistoryEntry` JSONL), restore-on-launch, unsaved-buffer stash | CLOUD | 0004, 0026 |
| PKT-0039 | `kate-ui`: search in files pane over `kate-search` + `Project` | LOCAL | 0030, 0023 |
| PKT-0040 | `kate-core`: more indenters — `python`, `cmake`, `xml` (one packet each) | EITHER | 0018 |
| PKT-0041 | `kate-ui`: settings window for `EditorConfig` | LOCAL | 0016 |
| PKT-0042 | Packaging: `.app` bundle, `Info.plist`, icon, file associations; signing and notarization are owner decisions | LOCAL | 0015 |

## Phase 4 — The big deferred ones

| Packet | Title | Lane | Notes |
|---|---|---|---|
| PKT-005x | `kate-vimode`: normal/insert/visual modes as a `Command` producer, keyparser, registers | CLOUD | 16k lines upstream; expect 10+ packets. |
| PKT-006x | Embedded terminal (`alacritty_terminal` + GPUI terminal view) | LOCAL | |
| PKT-007x | Snippets, templates, external tools, keyboard macros | mixed | |
| PKT-008x | Spell check via `NSSpellChecker` | LOCAL | |
| PKT-009x | LSP: semantic tokens, inlay hints, code actions, rename, formatting | CLOUD | |

## Issued after the plan

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0043 | Licensing and attribution. GPL-3.0-or-later, LGPL-2.1-or-later, MIT, SPDX headers. | LOCAL | PKT-0001 |

PKT-0002 through PKT-0013 depend on PKT-0043. They edit the same source
files. The license headers land first.

## Packet sizing rules the gatekeeper applies

- A packet touches one crate. Two crates need a reason in `summary` and are
  the exception (UI + core for a feature whose model half is under 50 lines).
- A packet's `definitions[]` fits on one screen. If the types do not fit, the
  packet is two packets.
- A packet has at least one regression test that fails without it, unless
  Gate 1 says `regression_test.required: false` (docs, bumps, vendoring).
- Vendoring data (definitions, themes, golden files) is never mixed with
  code in one packet; the diff must be reviewable as code *or* as data.
- A packet that creates a crate also adds its line to
  `[workspace.dependencies]` and nothing else to the root manifest.
