# Crates and boundaries

The workspace layout is the architecture. A packet's `allowed_files` is a
crate directory or files inside one; two packets in different crates cannot
collide; dependencies point one way. If a change needs two crates to move
together, that is two packets or a wrong boundary.

## The graph

```
                    ┌──────────────┐
                    │  katem5rust  │  apps/      the binary; main() only
                    └──────┬───────┘
                           │
                    ┌──────▼───────┐
                    │   kate-ui    │  GPUI shell: windows, menus, editor element, find bar,
                    └──────┬───────┘  tabs/splits, dialogs, file watcher, LSP wiring
                           │
        ┌──────────────────┼───────────────────┬──────────────────┐
        │                  │                   │                  │
 ┌──────▼──────┐    ┌──────▼──────┐     ┌──────▼──────┐    ┌──────▼──────┐
 │ kate-session│    │  kate-core  │     │  kate-lsp   │    │ kate-vimode │   (Phase 2–4)
 └──────┬──────┘    └──────┬──────┘     └──────┬──────┘    └──────┬──────┘
        │        ┌─────────┼──────────┐        │                  │
        │ ┌──────▼──────┐ ┌▼──────────▼┐ ┌─────▼───────┐          │
        │ │ kate-fileio │ │ kate-syntax│ │ kate-search │          │
        │ └─────────────┘ └────────────┘ └──────┬──────┘          │
        │                                       │                 │
        └───────────────────────┬───────────────┴─────────────────┘
                         ┌──────▼──────┐
                         │  kate-text  │   rope, edits, transactions, anchors, history
                         └─────────────┘
```

`kate-syntax` and `kate-fileio` depend on nothing internal. `kate-syntax`
takes `&str` lines and returns spans, which is why its golden tests run
against upstream's corpus without a buffer in sight. `kate-fileio` turns
bytes into a `ropey::Rope` and back; it needs the rope type, not the buffer.
Both crates can therefore be developed in lanes that never wait on
`kate-text`.

## The crates

| Crate | Exists now | Owns | Depends on (internal) | External deps (planned, each justified in its packet) |
|---|---|---|---|---|
| `kate-text` | yes (empty) | `Offset`, `CharRange`, `Revision`, `Edit`, `Transaction`, `TextBuffer`, `EditLog`, `Anchor`, `History` | — | `ropey` 1.6 (`default-features = false`, `simd`); dev: `proptest` |
| `kate-syntax` | yes (empty) | KSyntaxHighlighting definitions, engine, themes, vendored data | — | `quick-xml`, `fancy-regex`, `smallvec`, `lru`, `serde`, `serde_json` |
| `kate-search` | yes (empty) | `Pattern`, `SearchQuery`, `Match`, find/replace | `kate-text` | `regex`, `regex-cursor` |
| `kate-fileio` | yes (empty) | decode/encode, load/save, line endings, digest | — (produces and consumes `ropey::Rope` directly) | `ropey`, `encoding_rs`, `sha2`; dev: `tempfile` |
| `kate-core` | yes (empty) | `Selections`, movement, fan-out, `Document`, `View`, `Workspace`, `Command`, `Event`, `SyntaxCache`, marks, folding, indentation, modelines, config | `kate-text`, `kate-syntax`, `kate-fileio`, `kate-search` (from the find packet on) | `slotmap`, `unicode-segmentation`, `serde` (config) |
| `kate-session` | Phase 2 | `Session` JSON, `.kateproject` reader | `kate-core`, `kate-fileio` | `serde`, `serde_json` |
| `kate-lsp` | Phase 3 | JSON-RPC client, `ServerConfig`, position encoding, diagnostics | `kate-text`, `kate-core` | `lsp-types`, `serde_json` |
| `kate-vimode` | Phase 4 | vi input mode as a `Command` producer | `kate-core` | — |
| `kate-ui` | PKT-0013 | GPUI shell; all rendering; the event loop; file watcher | `kate-core`, `kate-session`, `kate-lsp` | `gpui`, `notify`, `serde` |
| `katem5rust` | yes | `main()` | `kate-ui` | — |
| `gatecheck` | yes | governance payload validation | — | `jsonschema` (no default features), `serde_json` |

Crates marked "Phase N" are created by the first packet that needs them, not
now. Five empty crates already exist because they are the boundaries the
first thirteen packets write into; creating `kate-vimode` today would be a
directory with a doc comment and a promise.

## Rules

1. **Dependencies flow down the graph only.** `kate-text` never imports
   anything from this workspace. `kate-core` never imports `kate-ui`. A crate
   that wants to go up the graph wants an `Event` instead.
2. **External dependencies are declared in the crate that uses them**, not in
   `[workspace.dependencies]`. Parallel packets in different crates then never
   touch the same manifest. `[workspace.dependencies]` lists internal path
   crates only, added by the packet that creates the crate.
3. **`unsafe_code = "forbid"` at the workspace level.** Every crate inherits
   `[lints] workspace = true`. GPUI does FFI inside GPUI.
4. **No `kate-common`, `kate-util`, `kate-types`.** Shared types live in the
   lowest crate that needs them (`kate-text` for positions). A utilities
   crate is where abstractions go to be speculative.
5. **The GUI crate owns no model state.** `kate-ui` holds layout caches keyed
   by `(line, Revision)` and nothing else. If a feature needs state, it
   belongs in `kate-core` where a test can reach it without a window.
6. **One public `lib.rs` per crate re-exports the API.** Internal modules
   are `pub(crate)` unless a packet says otherwise.

## Why these boundaries and not Kate's

KTextEditor is one library with `buffer/`, `undo/`, `document/`, `view/`,
`render/`, `syntax/`, `search/`, `vimode/`, `completion/`, `script/` as
directories, all in one compilation unit with private headers reaching
across. The app (`kate/apps/lib`) sits on top as a second library, and
addons as a third layer. Qt makes this work via `QObject` and signals.

Our cut is by *what can be tested without the thing above it*:

- `kate-text` is testable with strings.
- `kate-syntax` is testable with upstream's golden files.
- `kate-fileio` is testable with a temp directory.
- `kate-search` is testable with a buffer.
- `kate-core` is testable with all of the above and no window.
- `kate-ui` needs a Mac with a display, so it holds the least logic possible.

That is also exactly the local/cloud lane split in
`docs/governance/GOVERNANCE_LINUS.md` §VIII: everything above the line runs
in cloud lanes; `kate-ui` and APFS-specific save behavior run locally.
