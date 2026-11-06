# Backlog: sequenced work packets

Every packet is one Gate 1 payload, one issue, one branch, one squash
commit. Phase 0 and Phase 1 packets exist as JSON under `packets/` and
validate against `docs/governance/schemas/gate1-planning-handoff.schema.json`
(run `cargo run -p gatecheck`). Phase 2 and Phase 3 are JSON payloads under `packets/` too; their indexes
are the `WAVE-*.md` files named below. Phase 4 is listed as titles with
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

The first slices of PKT-0020, PKT-0021, PKT-0022, PKT-0023, PKT-0026, PKT-0027, and PKT-0029 are in `docs/design/WAVE-0020.md`.
PKT-0245 is the fold command that follows PKT-0020.
PKT-0024, PKT-0025, and PKT-0028 are cut in `docs/design/WAVE-0024.md`.
MIT copy wave 1 of 15 is in `docs/design/WAVE-0028.md`.
Eight files start at `abnf.xml`.
MIT copy wave 2 of 15 is in `docs/design/WAVE-0259.md`.
Eight files start at `cashscript.xml`.
MIT copy wave 3 of 15 is in `docs/design/WAVE-0267.md`.
Eight files start at `csv-semicolon.xml`.
MIT copy waves 4 to 15 are in `docs/design/WAVE-0275.md` through `docs/design/WAVE-0363.md`.
Wave 4 starts at `elixir-eex.xml`.
Wave 15 ends at `zig.xml`.
No MIT file stays queued after wave 15.
The include_str wiring is in `docs/design/WAVE-0367.md` through `docs/design/WAVE-0485.md`.
PKT-0367 moves the vendored list to `crates/kate-syntax/src/vendored.rs`.
PKT-0368 to PKT-0371 fix the engine for nine queued files.
PKT-0372 makes the window load every vendored definition.
PKT-0373 to PKT-0488 wire the 116 files, one packet per file, in copy order.
The lane split for the Cursor coding agents is in `docs/design/07-lane-split.md`.
The window remnants of Phase 2 are PKT-0489 to PKT-0511 in `docs/design/WAVE-0489.md`.
Phase 3 is planned: the titles PKT-0030 to PKT-0042 and the slices PKT-0512 to PKT-0555 are in `docs/design/WAVE-0030.md`, `WAVE-0031.md`, `WAVE-0033.md`, `WAVE-0034.md`, `WAVE-0035.md`, `WAVE-0038.md`, `WAVE-0039.md`, and `WAVE-0041.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0014 | `kate-ui`: editing input → `Command`, caret and selection painting, blink, scroll-to-cursor | LOCAL | 0013 |
| PKT-0015 | `kate-ui`: type, move, undo, paint every caret. Waits for PKT-0045. | LOCAL | 0013, 0045 |
| PKT-0016 | `kate-core`: EditorConfig TOML load and save. `document.*` is `default_document`. | CLOUD | 0012 |
| PKT-0017 | `kate-core`: kate modelines and `.editorconfig` on open. The modeline wins. | CLOUD | 0012 |
| PKT-0018 | `kate-core`: newline indent, plus Indent and Unindent. No key binding. | CLOUD | 0012 |
| PKT-0019 | `kate-core`: pair one typed bracket. Pairing is always on. | CLOUD | 0011 |
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

Every title below is planned as packet JSON.
The first slice of each title keeps its number; the other slices are PKT-0512 to PKT-0555.
The indexes are `docs/design/WAVE-0030.md` (0030), `WAVE-0031.md` (0031, 0032), `WAVE-0033.md` and `WAVE-0034.md` (0033, 0034), `WAVE-0035.md` (0035, 0036, 0037), `WAVE-0038.md` (0038), `WAVE-0039.md` (0039, 0040), and `WAVE-0041.md` (0041, 0042).
The lane column below is the lane of the title packet; the slices carry their own lane.

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

## Wave PKT-0053

These packets are cloud lanes.
They do not edit `kate-ui` or `apps`.
Their files do not overlap.
The index is `docs/design/WAVE-0053.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0053 | `kate-core`: unindent the other indent style | CLOUD | 0018 |
| PKT-0054 | `kate-core`: reject a replacement encoding label | CLOUD | 0016 |
| PKT-0055 | `kate-core`: reload reapplies the file mode | CLOUD | 0017, 0018 |
| PKT-0056 | `kate-core`: skip a bad `.editorconfig` file | CLOUD | 0017 |
| PKT-0057 | `kate-fileio`: encoding label lookup | CLOUD | 0005, 0006 |
| PKT-0058 | `kate-text`: bracket scan uses `chars_at` | CLOUD | 0052 |
| PKT-0059 | `kate-syntax`: cite the pinned syntax commit | CLOUD | 0008, 0044 |
| PKT-0060 | `kate-core`: case-insensitive subsequence score | CLOUD | 0012, 0016 |

## Wave PKT-0061

These packets are cloud lanes.
They do not edit the kate-ui files PKT-0015 is changing.
They do not edit `indent.rs`.
PKT-0053 is on main.
Their files do not overlap.
The index is `docs/design/WAVE-0061.md`.

Reload still drops per-document config edits made after open.
PKT-0055 states that behavior.
This wave does not add a packet to keep those edits.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0061 | `kate-core`: read `.editorconfig` from one file handle | CLOUD | 0056 |
| PKT-0062 | `kate-core`: saturate the fuzzy gap score | CLOUD | 0060 |
| PKT-0063 | `kate-core`: return reloaded text once | CLOUD | 0055 |
| PKT-0064 | `kate-core`: call `kate_fileio::encoding_for_label` | CLOUD | 0054, 0057 |
| PKT-0065 | `kate-core`: folds from line indent | CLOUD | 0012 |
| PKT-0066 | `kate-core`: word completions from the buffer | CLOUD | 0060 |
| PKT-0067 | `kate-search`: wrap flag on `FindBar` | CLOUD | 0050 |
| PKT-0068 | `kate-ui` layout: visual row count. Not session or paint. | CLOUD | 0013 |

## Wave PKT-0069

These packets leave PKT-0051 and PKT-0067 files unchanged.
PKT-0051 owns `files.rs`, `lib.rs`, and `session.rs`.
PKT-0067 owns `bar.rs`.
The `FindBar` wrap call stays in that file.
The `gpui-tests` feature stays an owner Mac step.
Their files do not overlap.
The index is `docs/design/WAVE-0069.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0069 | `kate-ui`: name a command error | CLOUD | 0013, 0068 |
| PKT-0070 | `kate-ui`: selection spans for the visible lines | CLOUD | 0013, 0015 |
| PKT-0071 | `kate-ui`: paint the selection after the line background | LOCAL | 0013, 0015, 0045 |
| PKT-0072 | `kate-syntax`: selection color off the line background | CLOUD | 0001, 0007, 0009 |
| PKT-0073 | `kate-core`: store indent folds on the view | CLOUD | 0012, 0065 |
| PKT-0074 | `kate-core`: insert a word completion | CLOUD | 0018, 0055, 0063, 0066 |
| PKT-0075 | `kate-core`: classify a change on disk | CLOUD | 0012, 0017 |
| PKT-0076 | `kate-core`: move to the matching bracket | CLOUD | 0011, 0052 |

## Wave PKT-0077

These packets leave the open PKT-0069, PKT-0071, PKT-0073, and PKT-0074 files unchanged.
The selection-span call and `selection_color` stay out of `paint.rs` until PKT-0071 lands.
The `gpui-tests` feature stays an owner Mac step.
Their files do not overlap.
The index is `docs/design/WAVE-0077.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0077 | `kate-syntax`: fallback selection color off the background | CLOUD | 0009, 0072 |
| PKT-0078 | `kate-ui`: `OpenFailed` when a new view is missing | CLOUD | 0051 |
| PKT-0079 | `kate-ui`: bind match-bracket and word completion | CLOUD | 0070, 0074, 0076 |
| PKT-0080 | `kate-core`: reload prompt from the disk change | CLOUD | 0075 |
| PKT-0081 | `kate-fileio`: open a regular file before reading it | CLOUD | 0005 |
| PKT-0082 | `kate-core`: complete words from earlier in the buffer | CLOUD | 0066 |
| PKT-0083 | `kate-core`: pair a typed backtick | CLOUD | 0019 |
| PKT-0084 | `kate-core`: indent a Python line that ends in a colon | CLOUD | 0018, 0053 |

## Wave PKT-0085

These packets leave `keys.rs` and `pair.rs` unchanged.
PKT-0079 and PKT-0083 still own those files.
The `gpui-tests` feature stays an owner Mac step.
Their files do not overlap.
The index is `docs/design/WAVE-0085.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0085 | `kate-syntax`: fallback selection color off the current line | CLOUD | 0072, 0077 |
| PKT-0086 | `kate-core`: Python indent from the text before the caret | CLOUD | 0084 |
| PKT-0087 | `kate-core`: indent fold range starts on the header line | CLOUD | 0073 |
| PKT-0088 | `kate-ui`: paint the selection in the theme selection color | LOCAL | 0071 |
| PKT-0089 | `kate-ui`: keep the sentence for a failed command | CLOUD | 0069 |
| PKT-0090 | `kate-core`: one sentence for a reload choice | CLOUD | 0080 |
| PKT-0091 | `kate-core`: reload prompt for one view | CLOUD | 0074, 0080 |
| PKT-0092 | `kate-core`: export `ReloadChoice` | CLOUD | 0080 |

## Wave PKT-0093

WAVE-0085 is on main.
`theme.rs`, `paint.rs`, `layout.rs`, and `session.rs` are free.
This wave still leaves those four files unchanged.
The `gpui-tests` feature stays an owner Mac step.
Their files do not overlap.
The index is `docs/design/WAVE-0093.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0093 | `kate-core`: fill indent folds after an edit | CLOUD | 0073, 0074, 0087, 0091 |
| PKT-0094 | `kate-core`: sentence for a file reload prompt | CLOUD | 0080, 0090 |
| PKT-0095 | `kate-ui`: bind indent, unindent, and reload | CLOUD | 0079 |
| PKT-0096 | `kate-ui`: return the reload prompt sentence | CLOUD | 0078, 0090, 0092 |
| PKT-0097 | `kate-core`: choose a newline from the indenter name | CLOUD | 0084, 0086 |
| PKT-0098 | `kate-search`: skip a find for an empty plain pattern | CLOUD | 0010, 0050 |
| PKT-0099 | `kate-core`: suffix a completion would insert | CLOUD | 0082 |
| PKT-0100 | `kate-syntax`: match a mime type in either case | CLOUD | 0007 |

## Wave PKT-0101

PKT-0093 is on main.
`workspace.rs` is free.
These packets still leave `workspace.rs`, `keys.rs`, and `repository.rs` unchanged.
PKT-0100 still owns `repository.rs`.
Each packet owns one file.
The `gpui-tests` feature stays an owner Mac step.
The index is `docs/design/WAVE-0101.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0101 | `kate-core`: name a deleted file in the reload sentence | CLOUD | 0090, 0094 |
| PKT-0102 | `kate-core`: offset inside an indent fold | CLOUD | 0073, 0087 |
| PKT-0103 | `kate-core`: indenter name in either case | CLOUD | 0097 |
| PKT-0104 | `kate-core`: skip a completion that copies the tail | CLOUD | 0099 |
| PKT-0105 | `kate-ui`: clamp a span window to the buffer | CLOUD | 0068, 0089 |
| PKT-0106 | `kate-core`: accept yes and no in a modeline | CLOUD | 0017, 0061 |
| PKT-0107 | `kate-core`: offset inside a selection | CLOUD | 0011 |
| PKT-0108 | `kate-ui`: name an open or save failure | CLOUD | 0078, 0096 |

## Wave PKT-0109

WAVE-0101 is on main.
A local Qwen3.8 coder model with 27B parameters codes every packet on the owner Mac.
Each packet is one file.
Each change stays under about 60 lines.
The `gpui-tests` feature stays an owner Mac step.
The index is `docs/design/WAVE-0109.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0109 | `kate-ui`: leave a live view after the last close | LOCAL | 0108 |
| PKT-0110 | `kate-core`: shift folds on a same-line insert | LOCAL | 0093 |
| PKT-0111 | `kate-core`: open an editorconfig without blocking | LOCAL | 0106 |
| PKT-0112 | `kate-ui`: show an open or save failure | LOCAL | 0089 |
| PKT-0113 | `kate-core`: name a file that came back | LOCAL | 0101 |
| PKT-0114 | `kate-ui`: bind ctrl-s to save | LOCAL | 0095 |
| PKT-0115 | `kate-core`: test an offset against every selection | LOCAL | 0107 |
| PKT-0116 | `kate-search`: skip a find for an empty escape | LOCAL | 0098 |

## Wave PKT-0117

WAVE-0109 is on main.
A local Qwen3.8 coder model with 27B parameters codes every packet on the owner Mac.
Each packet is one file.
Each change stays under about 60 lines.
Add each test inside the one existing test module.
Do not add a second `mod tests`.
The index is `docs/design/WAVE-0117.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0117 | `kate-core`: newline uses the indenter name | LOCAL | 0110 |
| PKT-0118 | `kate-ui`: bind option-arrow to word movement | LOCAL | 0114 |
| PKT-0119 | `kate-search`: skip a find for an empty regex | LOCAL | 0116 |
| PKT-0120 | `kate-core`: dedent a Python return line | LOCAL | 0103 |
| PKT-0121 | `kate-text`: match angle brackets | LOCAL | 0058 |
| PKT-0122 | `kate-core`: pick the innermost indent fold | LOCAL | 0073 |
| PKT-0123 | `kate-core`: return the fold range at an offset | LOCAL | 0102 |
| PKT-0124 | `kate-core`: score a camel-case hump | LOCAL | 0060 |

## Wave PKT-0125

WAVE-0117 is on main.
A local Qwen3.8 coder model with 27B parameters codes every packet on the owner Mac.
Each packet is one file.
Each change stays under about 60 lines.
Add each test inside the one existing test module.
`replace.rs` gets one new `mod tests`.
Do not add a second `mod tests`.
Each packet lists the nearby functions that stay unchanged.
The index is `docs/design/WAVE-0125.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0125 | `kate-ui`: bind ctrl-z to undo | LOCAL | 0118 |
| PKT-0126 | `kate-core`: keep a quote after a backslash plain | LOCAL | 0083 |
| PKT-0127 | `kate-search`: skip replace-all for an empty pattern | LOCAL | 0010 |
| PKT-0128 | `kate-core`: a changed file after delete is modified | LOCAL | 0113 |
| PKT-0129 | `kate-ui`: count a tab stop in a wrapped line | LOCAL | 0105 |
| PKT-0130 | `kate-fileio`: write one ending for a CR LF pair | LOCAL | 0006 |
| PKT-0131 | `kate-ui`: keep catch-up coloring inside the viewport | LOCAL | 0045 |
| PKT-0132 | `kate-core`: dedent a bare Python else | LOCAL | 0120 |

## Wave PKT-0133

WAVE-0125 is in review.
This wave leaves `keys.rs`, `pair.rs`, `replace.rs`, `document.rs`, `layout.rs`, `encode.rs`, `highlight.rs`, and `indent.rs` unchanged.
A local Qwen3.8 coder model with 27B parameters codes every packet on the owner Mac.
Each packet is one file.
Each change stays under about 60 lines.
Add each test inside the one existing test module.
`literal.rs` gets one new `mod tests`.
Do not add a second `mod tests`.
Each packet lists the nearby functions that stay unchanged.
The index is `docs/design/WAVE-0133.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0133 | `kate-fileio`: open a load without blocking | LOCAL | 0081 |
| PKT-0134 | `kate-search`: keep an apostrophe inside a word | LOCAL | 0010 |
| PKT-0135 | `kate-core`: complete a word that contains an apostrophe | LOCAL | 0104 |
| PKT-0136 | `kate-core`: score a digit before a capital | LOCAL | 0124 |
| PKT-0137 | `kate-core`: count the folds that cover an offset | LOCAL | 0123 |
| PKT-0138 | `kate-core`: test a line inside a fold span | LOCAL | 0122 |
| PKT-0139 | `kate-search`: count hits and skip an empty pattern | LOCAL | 0067 |
| PKT-0140 | `kate-fileio`: refuse a save onto a directory | LOCAL | 0006 |

## Wave PKT-0141

WAVE-0133 code is in review.
This wave leaves `load.rs`, `save.rs`, `literal.rs`, `bar.rs`, `completion.rs`, `fuzzy.rs`, `view.rs`, and `fold.rs` unchanged.
A local Qwen3.8 coder model with 27B parameters codes every packet on the owner Mac.
Each packet is one file.
Each change stays under about 60 lines.
Add each test inside the one existing test module.
Do not add a second `mod tests`.
Each packet lists the nearby functions that stay unchanged.
A nested condition is written as `if a && b` or one `if else if`.
The index is `docs/design/WAVE-0141.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0141 | `kate-ui`: bind ctrl-a and ctrl-e to the line edge | LOCAL | 0125 |
| PKT-0142 | `kate-core`: wrap a quote after an even backslash run | LOCAL | 0126 |
| PKT-0143 | `kate-search`: skip a replacement that copies the hit | LOCAL | 0127 |
| PKT-0144 | `kate-fileio`: treat a trailing CR as an ending | LOCAL | 0130 |
| PKT-0145 | `kate-text`: name the other side of a bracket | LOCAL | 0121 |
| PKT-0146 | `kate-core`: name a breakpoint mark | LOCAL | 0128 |
| PKT-0147 | `kate-text`: return none for a scalar past the end | LOCAL | 0003 |
| PKT-0148 | `kate-ui`: keep color when the paint budget is zero | LOCAL | 0131 |

## Wave PKT-0149

WAVE-0141 code is in review.
This wave leaves `keys.rs`, `pair.rs`, `replace.rs`, `encode.rs`, `brackets.rs`, `document.rs`, `buffer.rs`, and `highlight.rs` unchanged.
A local Qwen3.8 coder model with 27B parameters codes every packet on the owner Mac.
Each packet is one file.
Each change stays under about 60 lines.
Add each test inside the one existing test module.
Do not add a second `mod tests`.
Each packet lists the nearby functions that stay unchanged.
A nested condition is written as `if a && b` or one `if else if`.
The index is `docs/design/WAVE-0149.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0149 | `kate-core`: keep an apostrophe inside a word move | LOCAL | 0076 |
| PKT-0150 | `kate-core`: indent after a brace before a line comment | LOCAL | 0132 |
| PKT-0151 | `kate-core`: score a letter after a digit | LOCAL | 0136 |
| PKT-0152 | `kate-ui`: give a carriage return no width | LOCAL | 0129 |
| PKT-0153 | `kate-core`: keep a hash comment inside a fold | LOCAL | 0138 |
| PKT-0154 | `kate-core`: accept yes and no as booleans | LOCAL | 0064 |
| PKT-0155 | `kate-core`: accept lf, crlf, and cr in a modeline | LOCAL | 0111 |
| PKT-0156 | `kate-search`: skip a one-hit replace that copies the hit | LOCAL | 0139 |

## Wave PKT-0157

WAVE-0149 code is in review.
This wave leaves `movement.rs`, `indent.rs`, `fuzzy.rs`, `layout.rs`, `fold.rs`, `config.rs`, `modeline.rs`, and `bar.rs` unchanged.
A local Qwen3.8 coder model with 27B parameters codes every packet on the owner Mac.
Each packet is one file.
Each change stays under about 60 lines.
Add each test inside the one existing test module.
Do not add a second `mod tests`.
Each packet lists the nearby functions that stay unchanged.
A nested condition is written as `if a && b` or one `if else if`.
The index is `docs/design/WAVE-0157.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0157 | `kate-ui`: bind ctrl-home and ctrl-end to the document edge | LOCAL | 0141 |
| PKT-0158 | `kate-core`: keep a quote plain after any letter | LOCAL | 0142 |
| PKT-0159 | `kate-core`: complete a word in either ASCII case | LOCAL | 0135 |
| PKT-0160 | `kate-core`: name a diagnostic mark | LOCAL | 0146 |
| PKT-0161 | `kate-core`: tell a forward selection from a backward one | LOCAL | 0115 |
| PKT-0162 | `kate-text`: return the last scalar | LOCAL | 0147 |
| PKT-0163 | `kate-text`: tell a bracket from other scalars | LOCAL | 0145 |
| PKT-0164 | `kate-search`: skip an empty whole-word find | LOCAL | 0134 |

## Wave PKT-0165

WAVE-0157 code is in review.
This wave leaves `keys.rs`, `pair.rs`, `completion.rs`, `document.rs`, `selection.rs`, `buffer.rs`, `brackets.rs`, and `literal.rs` unchanged.
A local Qwen3.8 coder model with 27B parameters codes every packet on the owner Mac.
Each packet is one file.
Each change stays under about 60 lines.
Add each test inside the one existing test module.
Do not add a second `mod tests`.
Each packet lists the nearby functions that stay unchanged.
A nested condition is written as `if a && b` or one `if else if`.
Spaces and tabs use `[' ', '\t']`.
The index is `docs/design/WAVE-0165.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0165 | `kate-core`: keep a word move on an apostrophe | LOCAL | 0149 |
| PKT-0166 | `kate-core`: indent after a colon before a hash comment | LOCAL | 0150 |
| PKT-0167 | `kate-core`: keep a slash comment inside a fold | LOCAL | 0153 |
| PKT-0168 | `kate-ui`: give a newline no width | LOCAL | 0152 |
| PKT-0169 | `kate-core`: accept on and off as booleans | LOCAL | 0154 |
| PKT-0170 | `kate-core`: accept any letter case in a modeline ending | LOCAL | 0155 |
| PKT-0171 | `kate-ui`: start plain text at the checkpoint | LOCAL | 0148 |
| PKT-0172 | `kate-ui`: name an edit that misses the buffer | LOCAL | 0013 |

## Wave PKT-0173

WAVE-0165 code is in review.
This wave leaves `movement.rs`, `indent.rs`, `fold.rs`, `layout.rs`, `config.rs`, `modeline.rs`, `highlight.rs`, and `notice.rs` unchanged.
A local Qwen3.8 coder model with 27B parameters codes every packet on the owner Mac.
Each packet is one file.
Each change stays under about 60 lines.
Add each test inside the one existing test module.
Do not add a second `mod tests`.
Each packet lists the nearby functions that stay unchanged.
A nested condition is written as `if a && b` or one `if else if`.
Spaces and tabs use `[' ', '\t']`.
A packet that changes a documented rule names the tests and comments that may change.
The index is `docs/design/WAVE-0173.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0173 | `kate-ui`: bind ctrl-d and ctrl-h to delete | LOCAL | 0157 |
| PKT-0174 | `kate-core`: complete a word that contains a Unicode letter | LOCAL | 0159 |
| PKT-0175 | `kate-core`: tell a backward selection from a caret | LOCAL | 0161 |
| PKT-0176 | `kate-fileio`: treat a Unicode line break as an ending | LOCAL | 0144 |
| PKT-0177 | `kate-fileio`: store a Unicode line break as LF | LOCAL | 0005 |
| PKT-0178 | `kate-core`: score a letter after any numeric scalar | LOCAL | 0151 |
| PKT-0179 | `kate-text`: tell an insert from a delete | LOCAL | 0002 |
| PKT-0180 | `kate-core`: name a bookmark mark | LOCAL | 0160 |

## Wave PKT-0181

WAVE-0173 code is in review.
This wave leaves `keys.rs`, `completion.rs`, `selection.rs`, `encode.rs`, `decode.rs`, `fuzzy.rs`, `edit.rs`, and `document.rs` unchanged.
A local Qwen3.8 coder model with 27B parameters codes every packet on the owner Mac.
Each packet is one file.
Each change stays under about 60 lines.
Add each test inside the one existing test module.
Do not add a second `mod tests`.
Each packet lists the nearby functions that stay unchanged.
A nested condition is written as `if a && b` or one `if else if`.
Spaces and tabs use `[' ', '\t']`.
A packet that changes a documented rule names the tests and comments that may change.
The index is `docs/design/WAVE-0181.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0181 | `kate-core`: indent after a brace before a block comment | LOCAL | 0166 |
| PKT-0182 | `kate-core`: keep a block comment inside a fold | LOCAL | 0167 |
| PKT-0183 | `kate-ui`: give a Unicode line break no width | LOCAL | 0168 |
| PKT-0184 | `kate-core`: accept 1 and 0 as booleans | LOCAL | 0169 |
| PKT-0185 | `kate-core`: accept any letter case in a modeline flag | LOCAL | 0170 |
| PKT-0186 | `kate-text`: return the scalar before an offset | LOCAL | 0162 |
| PKT-0187 | `kate-text`: tell an opening bracket from a closer | LOCAL | 0163 |
| PKT-0188 | `kate-core`: tell when a view has scrolled | LOCAL | 0137 |

## Wave PKT-0189

WAVE-0181 plans are on main.
This wave leaves `indent.rs`, `fold.rs`, `layout.rs`, `config.rs`, `modeline.rs`, `buffer.rs`, `brackets.rs`, and `view.rs` unchanged.
WAVE-0173 code is on main.
This wave leaves `keys.rs`, `completion.rs`, `selection.rs`, `encode.rs`, `decode.rs`, `fuzzy.rs`, `edit.rs`, and `document.rs` unchanged.
A local Qwen3.8 coder model with 27B parameters codes every packet on the owner Mac.
Each packet is one file.
Each change stays under about 60 lines.
Add each test inside the one existing test module.
Do not add a second `mod tests`.
Each packet lists the nearby functions that stay unchanged.
A nested condition is written as `if a && b` or one `if else if`.
Two arms that return the same value use one `||` condition.
Spaces and tabs use `[' ', '\t']`.
A packet that changes a documented rule names the tests and comments that may change.
The index is `docs/design/WAVE-0189.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0189 | `kate-core`: keep a curly apostrophe inside a word move | LOCAL | 0165 |
| PKT-0190 | `kate-search`: treat a curly apostrophe as a word character | LOCAL | 0164 |
| PKT-0191 | `kate-core`: keep a quote plain after an apostrophe | LOCAL | 0158 |
| PKT-0192 | `kate-text`: tell when two ranges share a scalar | LOCAL | 0002 |
| PKT-0193 | `kate-fileio`: name a load that is not a file | LOCAL | 0133 |
| PKT-0194 | `kate-fileio`: name a save that is not a file | LOCAL | 0140 |
| PKT-0195 | `kate-ui`: tell when a highlight step needs paint | LOCAL | 0171 |
| PKT-0196 | `kate-ui`: map a gutter channel to a byte | LOCAL | 0013 |

## Wave PKT-0197

WAVE-0189 plans are on main.
This wave leaves `movement.rs`, `literal.rs`, `pair.rs`, `offset.rs`, `load.rs`, `save.rs`, `highlight.rs`, and `cache.rs` unchanged.
WAVE-0181 code is on main.
This wave leaves `indent.rs`, `fold.rs`, `layout.rs`, `config.rs`, `modeline.rs`, `buffer.rs`, `brackets.rs`, and `view.rs` unchanged.
A local Qwen3.8 coder model with 27B parameters codes every packet on the owner Mac.
Each packet is one file.
Each change stays under about 60 lines.
Add each test inside the one existing test module.
Do not add a second `mod tests`.
Each packet lists the nearby functions that stay unchanged.
A nested condition is written as `if a && b` or one `if else if`.
Two arms that return the same value use one `||` condition.
Spaces and tabs use `[' ', '\t']`.
A packet that changes a documented rule names the tests and comments that may change.
The index is `docs/design/WAVE-0197.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0197 | `kate-core`: complete a word across a curly apostrophe | LOCAL | 0174 |
| PKT-0198 | `kate-core`: drop a boundary bonus after a letter | LOCAL | 0178 |
| PKT-0199 | `kate-ui`: bind shift-tab to unindent | LOCAL | 0173 |
| PKT-0200 | `kate-text`: tell a delete from a replacement | LOCAL | 0179 |
| PKT-0201 | `kate-syntax`: read a four-digit theme color | LOCAL | 0085 |
| PKT-0202 | `kate-syntax`: keep a single case flag | LOCAL | 0008 |
| PKT-0203 | `kate-ui`: accept true for a profile line | LOCAL | 0013 |
| PKT-0204 | `kate-core`: tell an execution mark | LOCAL | 0180 |

## Wave PKT-0205

WAVE-0197 plans are on main.
This wave leaves `completion.rs`, `fuzzy.rs`, `keys.rs`, `edit.rs`, `theme.rs`, `regex.rs`, `profile.rs`, and `document.rs` unchanged.
WAVE-0189 code is on main.
This wave leaves `movement.rs`, `literal.rs`, `pair.rs`, `offset.rs`, `load.rs`, `save.rs`, `highlight.rs`, and `cache.rs` unchanged.
A local Qwen3.8 coder model with 27B parameters codes every packet on the owner Mac.
Each packet is one file.
Each change stays under about 60 lines.
Add each test inside the one existing test module.
Do not add a second `mod tests`.
Each packet lists the nearby functions that stay unchanged.
A nested condition is written as `if a && b` or one `if else if`.
Two arms that return the same value use one `||` condition.
Spaces and tabs use `[' ', '\t']`.
A packet that changes a documented rule names the tests and comments that may change.
The index is `docs/design/WAVE-0205.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0205 | `kate-syntax`: match a file name in either case | LOCAL | 0100 |
| PKT-0206 | `kate-core`: count a carriage return as one line | LOCAL | 0012 |
| PKT-0207 | `kate-text`: tell a closing bracket from an opener | LOCAL | 0187 |
| PKT-0208 | `kate-text`: return the scalar after an offset | LOCAL | 0186 |
| PKT-0209 | `kate-core`: accept any case in a trailing-space word | LOCAL | 0185 |
| PKT-0210 | `kate-core`: accept any case in a config boolean | LOCAL | 0184 |
| PKT-0211 | `kate-core`: fold a line that a carriage return ends | LOCAL | 0188 |
| PKT-0212 | `kate-core`: count the scalars in a selection | LOCAL | 0175 |

## Wave PKT-0213

WAVE-0205 plans are on main.
This wave leaves `repository.rs`, `syntax_cache.rs`, `brackets.rs`, `buffer.rs`, `modeline.rs`, `config.rs`, `view.rs`, and `selection.rs` unchanged.
WAVE-0197 code is on main.
This wave leaves `completion.rs`, `edit.rs`, `theme.rs`, `regex.rs`, `profile.rs`, and `document.rs` unchanged.
`fuzzy.rs` and `keys.rs` each get one new behavior on a different function.
A local Qwen3.8 coder model with 27B parameters codes every packet on the owner Mac.
Each packet is one file.
Each change stays under about 60 lines.
Add each test inside the one existing test module.
Do not add a second `mod tests`.
Each packet lists the nearby functions that stay unchanged.
A nested condition is written as `if a && b` or one `if else if`.
Two arms that return the same value use one `||` condition.
Spaces and tabs use `[' ', '\t']`.
A packet that changes a documented rule names the tests and comments that may change.
The index is `docs/design/WAVE-0213.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0213 | `kate-ui`: give a zero-width space no width | LOCAL | 0183 |
| PKT-0214 | `kate-fileio`: count a lone CR in a line position | LOCAL | 0176 |
| PKT-0215 | `kate-search`: skip an empty hit in the count | LOCAL | 0156 |
| PKT-0216 | `kate-text`: tell when two ranges touch | LOCAL | 0192 |
| PKT-0217 | `kate-core`: score a camel hump on a unicode letter | LOCAL | 0198 |
| PKT-0218 | `kate-ui`: treat meta and command as cmd | LOCAL | 0199 |
| PKT-0219 | `kate-text`: report how many undos are stored | LOCAL | 0004 |
| PKT-0220 | `kate-core`: skip a byte-order mark at smart home | LOCAL | 0189 |

## Wave PKT-0221

WAVE-0213 plans are on main.
This wave leaves `layout.rs`, `encode.rs`, `bar.rs`, `offset.rs`, `fuzzy.rs`, `keys.rs`, `history.rs`, and `movement.rs` unchanged.
`view.rs` stays unchanged while PKT-0211 is in flight.
A local Qwen3.8 coder model with 27B parameters codes every packet on the owner Mac.
Each packet is one file.
Each change stays under about 60 lines.
Add each test inside the one existing test module.
Do not add a second `mod tests`.
Each packet lists the nearby functions that stay unchanged.
A nested condition is written as `if a && b` or one `if else if`.
Two arms that return the same value use one `||` condition.
Spaces and tabs use `[' ', '\t']`.
A packet that changes a documented rule names the tests and comments that may change.
The index is `docs/design/WAVE-0221.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0221 | `kate-core`: complete a word in either letter case | LOCAL | 0197 |
| PKT-0222 | `kate-core`: pair a curly double quote | LOCAL | 0191 |
| PKT-0223 | `kate-core`: indent after a fullwidth colon | LOCAL | 0181 |
| PKT-0224 | `kate-core`: keep a no-break space inside a fold | LOCAL | 0182 |
| PKT-0225 | `kate-search`: keep a soft hyphen inside a word | LOCAL | 0190 |
| PKT-0226 | `kate-text`: tell a replacement from an insert | LOCAL | 0200 |
| PKT-0227 | `kate-ui`: accept TRUE for a profile line | LOCAL | 0203 |
| PKT-0228 | `kate-syntax`: fold a capital I in a case flag | LOCAL | 0202 |

## Wave PKT-0229

WAVE-0221 plans are on main.
This wave leaves `completion.rs`, `pair.rs`, `indent.rs`, `fold.rs`, `literal.rs`, `edit.rs`, `profile.rs`, and `regex.rs` unchanged.
WAVE-0213 code is on main.
This wave leaves those eight files unchanged as well.
A local Qwen3.8 coder model with 27B parameters codes every packet on the owner Mac.
Each packet is one file.
Each change stays under about 60 lines.
Add each test inside the one existing test module.
Do not add a second `mod tests`.
Each packet lists the nearby functions that stay unchanged.
A nested condition is written as `if a && b` or one `if else if`.
Two arms that return the same value use one `||` condition.
Spaces and tabs use `[' ', '\t']`.
A packet that changes a documented rule names the tests and comments that may change.
The index is `docs/design/WAVE-0229.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0229 | `kate-text`: match a fullwidth parenthesis | LOCAL | 0207 |
| PKT-0230 | `kate-fileio`: accept a hyphen in an encoding label | LOCAL | 0177 |
| PKT-0231 | `kate-syntax`: match an accented file extension | LOCAL | 0205 |
| PKT-0232 | `kate-core`: accept a hyphen in a modeline encoding | LOCAL | 0209 |
| PKT-0233 | `kate-syntax`: trim spaces around a theme color | LOCAL | 0201 |
| PKT-0234 | `kate-syntax`: read a fullwidth digit as an integer | LOCAL | 0008 |
| PKT-0235 | `kate-fileio`: name an empty load path | LOCAL | 0193 |
| PKT-0236 | `kate-core`: tell an error mark from a warning | LOCAL | 0204 |

## Wave PKT-0020

This wave is the first Phase 2 plan.
It leaves multi-cursor, tabs, and syntax vendoring for later waves.
A local Qwen3.8 coder model with 27B parameters codes every packet on the owner Mac.
Each packet is one file.
Each change stays under about 60 lines.
Add each test inside the one existing test module.
Do not add a second `mod tests`.
PKT-0245 waits for PKT-0020 because both edit `fold.rs`.
The other six packets use different files and can start together.
The index is `docs/design/WAVE-0020.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0020 | `kate-core`: build fold regions from syntax events | LOCAL | 0224 |
| PKT-0021 | `kate-ui`: step one visual row on a wrapped line | LOCAL | 0213 |
| PKT-0022 | `kate-ui`: record the gutter columns for one line | LOCAL | 0218 |
| PKT-0023 | `kate-search`: list the hits a find bar paints | LOCAL | 0215 |
| PKT-0026 | `kate-ui`: round-trip open paths as session JSON | LOCAL | 0227 |
| PKT-0027 | `kate-ui`: name the reload and keep answers | LOCAL | 0172 |
| PKT-0029 | `kate-ui`: choose the light or dark theme name | LOCAL | 0196 |
| PKT-0245 | `kate-core`: fold the region at the caret line | LOCAL | 0020 |

## Wave PKT-0024

WAVE-0020 code is on main.
This wave cuts the remaining Phase 2 titles.
Phase 3 stays titles.
A local Qwen3.8 coder model with 27B parameters codes every packet on the owner Mac.
Each packet is one file.
Each change stays under about 60 lines.
Add each test inside the one existing test module.
Do not add a second `mod tests`.
PKT-0024, PKT-0025, PKT-0028, and PKT-0249 can start together.
PKT-0246 waits for PKT-0024.
PKT-0247 waits for PKT-0025.
PKT-0250 waits for PKT-0247.
PKT-0248 waits for PKT-0025.
The index is `docs/design/WAVE-0024.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0024 | `kate-core`: add a caret at an offset | LOCAL | 0212 |
| PKT-0025 | `kate-core`: split the tabs after one view | LOCAL | 0117 |
| PKT-0028 | `kate-syntax`: refuse a definition that is not MIT | LOCAL | 0231 |
| PKT-0246 | `kate-core`: select the next copy of the selection | LOCAL | 0024 |
| PKT-0247 | `kate-core`: make one tab the active tab | LOCAL | 0025 |
| PKT-0248 | `kate-ui`: remember a side-by-side session | LOCAL | 0025 |
| PKT-0249 | `kate-ui`: choose the tab that stays after a close | LOCAL | 0021 |
| PKT-0250 | `kate-core`: list the tabs from left to right | LOCAL | 0247 |

## Wave PKT-0028

This is MIT copy wave 1 of 15.
Eight syntax files are copied from KSyntaxHighlighting `v6.31.0`.
108 MIT files remain queued.
The `include_str` packets wait until these eight files are on main.
Phase 3 stays titles.
The index is `docs/design/WAVE-0028.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0251 | `kate-syntax`: copy the MIT ABNF definition | LOCAL | 0028 |
| PKT-0252 | `kate-syntax`: copy the MIT Adblock Plus definition | LOCAL | 0028 |
| PKT-0253 | `kate-syntax`: copy the MIT AHDL definition | LOCAL | 0028 |
| PKT-0254 | `kate-syntax`: copy the MIT ANTLR definition | LOCAL | 0028 |
| PKT-0255 | `kate-syntax`: copy the MIT ARM Assembler definition | LOCAL | 0028 |
| PKT-0256 | `kate-syntax`: copy the MIT BrightScript definition | LOCAL | 0028 |
| PKT-0257 | `kate-syntax`: copy the MIT Cabal definition | LOCAL | 0028 |
| PKT-0258 | `kate-syntax`: copy the MIT Cap'n Proto definition | LOCAL | 0028 |

## Wave PKT-0259

This is MIT copy wave 2 of 15.
Eight syntax files are copied from KSyntaxHighlighting `v6.31.0`.
100 MIT files remain queued.
A Cursor cloud agent copies each file.
The owner Mac stays out of this wave.
The `include_str` packets wait until these eight files are on main.
Phase 3 stays titles.
Packet numbers 0237 through 0244 stay unused.
The index is `docs/design/WAVE-0259.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0259 | `kate-syntax`: copy the MIT CashScript definition | CLOUD | 0028 |
| PKT-0260 | `kate-syntax`: copy the MIT ChangeLog definition | CLOUD | 0028 |
| PKT-0261 | `kate-syntax`: copy the MIT Common Intermediate Language (CIL) definition | CLOUD | 0028 |
| PKT-0262 | `kate-syntax`: copy the MIT CLIST definition | CLOUD | 0028 |
| PKT-0263 | `kate-syntax`: copy the MIT COBOL definition | CLOUD | 0028 |
| PKT-0264 | `kate-syntax`: copy the MIT CoffeeScript definition | CLOUD | 0028 |
| PKT-0265 | `kate-syntax`: copy the MIT Common Lisp definition | CLOUD | 0028 |
| PKT-0266 | `kate-syntax`: copy the MIT CSV (pipe) definition | CLOUD | 0028 |

## Wave PKT-0267

This is MIT copy wave 3 of 15.
Eight syntax files are copied from KSyntaxHighlighting `v6.31.0`.
92 MIT files remain queued.
A Cursor cloud agent copies each file.
The owner Mac stays out of this wave.
The `include_str` packets wait until these eight files are on main.
Phase 3 stays titles.
Packet numbers 0237 through 0244 stay unused.
The index is `docs/design/WAVE-0267.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0267 | `kate-syntax`: copy the MIT CSV (semicolon) definition | CLOUD | 0028 |
| PKT-0268 | `kate-syntax`: copy the MIT CSV (whitespace) definition | CLOUD | 0028 |
| PKT-0269 | `kate-syntax`: copy the MIT D2 definition | CLOUD | 0028 |
| PKT-0270 | `kate-syntax`: copy the MIT Dart definition | CLOUD | 0028 |
| PKT-0271 | `kate-syntax`: copy the MIT Dockerfile definition | CLOUD | 0028 |
| PKT-0272 | `kate-syntax`: copy the MIT DotEnv definition | CLOUD | 0028 |
| PKT-0273 | `kate-syntax`: copy the MIT Doxygen definition | CLOUD | 0028 |
| PKT-0274 | `kate-syntax`: copy the MIT Earthfile definition | CLOUD | 0028 |

## Wave PKT-0275

This is MIT copy wave 4 of 15.
Eight syntax files are copied from KSyntaxHighlighting `v6.31.0`.
84 MIT files remain queued.
A Cursor local agent on the Framework machine copies each file.
The owner Mac stays out of this wave.
The wiring packets are PKT-0397 through PKT-0404.
Packet numbers 0237 through 0244 stay unused.
The index is `docs/design/WAVE-0275.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0275 | `kate-syntax`: copy the MIT Elixir/EEx definition | LOCAL | 0028 |
| PKT-0276 | `kate-syntax`: copy the MIT Elixir/HEEx definition | LOCAL | 0028 |
| PKT-0277 | `kate-syntax`: copy the MIT Elm definition | LOCAL | 0028 |
| PKT-0278 | `kate-syntax`: copy the MIT Email definition | LOCAL | 0028 |
| PKT-0279 | `kate-syntax`: copy the MIT Expect definition | LOCAL | 0028 |
| PKT-0280 | `kate-syntax`: copy the MIT FlatBuffers definition | LOCAL | 0028 |
| PKT-0281 | `kate-syntax`: copy the MIT Fluent definition | LOCAL | 0028 |
| PKT-0282 | `kate-syntax`: copy the MIT Fortran (Free Format) definition | LOCAL | 0028 |

## Wave PKT-0283

This is MIT copy wave 5 of 15.
Eight syntax files are copied from KSyntaxHighlighting `v6.31.0`.
76 MIT files remain queued.
A Cursor local agent on the Framework machine copies each file.
The owner Mac stays out of this wave.
The wiring packets are PKT-0405 through PKT-0412.
Packet numbers 0237 through 0244 stay unused.
The index is `docs/design/WAVE-0283.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0283 | `kate-syntax`: copy the MIT GDL definition | LOCAL | 0028 |
| PKT-0284 | `kate-syntax`: copy the MIT Gemtext definition | LOCAL | 0028 |
| PKT-0285 | `kate-syntax`: copy the MIT GNU Gettext definition | LOCAL | 0028 |
| PKT-0286 | `kate-syntax`: copy the MIT Cucumber Gherkin feature definition | LOCAL | 0028 |
| PKT-0287 | `kate-syntax`: copy the MIT Gleam definition | LOCAL | 0028 |
| PKT-0288 | `kate-syntax`: copy the MIT GN definition | LOCAL | 0028 |
| PKT-0289 | `kate-syntax`: copy the MIT GPRBuild definition | LOCAL | 0028 |
| PKT-0290 | `kate-syntax`: copy the MIT GraphQL definition | LOCAL | 0028 |

## Wave PKT-0291

This is MIT copy wave 6 of 15.
Eight syntax files are copied from KSyntaxHighlighting `v6.31.0`.
68 MIT files remain queued.
A Cursor local agent on the Framework machine copies each file.
The owner Mac stays out of this wave.
The wiring packets are PKT-0413 through PKT-0420.
Packet numbers 0237 through 0244 stay unused.
The index is `docs/design/WAVE-0291.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0291 | `kate-syntax`: copy the MIT GTK Blueprint definition | LOCAL | 0028 |
| PKT-0292 | `kate-syntax`: copy the MIT Hare definition | LOCAL | 0028 |
| PKT-0293 | `kate-syntax`: copy the MIT Haxe definition | LOCAL | 0028 |
| PKT-0294 | `kate-syntax`: copy the MIT Hjson definition | LOCAL | 0028 |
| PKT-0295 | `kate-syntax`: copy the MIT InnoSetup definition | LOCAL | 0028 |
| PKT-0296 | `kate-syntax`: copy the MIT Intel HEX definition | LOCAL | 0028 |
| PKT-0297 | `kate-syntax`: copy the MIT Java Module definition | LOCAL | 0028 |
| PKT-0298 | `kate-syntax`: copy the MIT Java Properties definition | LOCAL | 0028 |

## Wave PKT-0299

This is MIT copy wave 7 of 15.
Eight syntax files are copied from KSyntaxHighlighting `v6.31.0`.
60 MIT files remain queued.
A Cursor local agent on the Framework machine copies each file.
The owner Mac stays out of this wave.
The wiring packets are PKT-0421 through PKT-0428.
Packet numbers 0237 through 0244 stay unused.
The index is `docs/design/WAVE-0299.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0299 | `kate-syntax`: copy the MIT JCL definition | LOCAL | 0028 |
| PKT-0300 | `kate-syntax`: copy the MIT Jinja definition | LOCAL | 0028 |
| PKT-0301 | `kate-syntax`: copy the MIT JSON5 definition | LOCAL | 0028 |
| PKT-0302 | `kate-syntax`: copy the MIT Jsonnet definition | LOCAL | 0028 |
| PKT-0303 | `kate-syntax`: copy the MIT Just definition | LOCAL | 0028 |
| PKT-0304 | `kate-syntax`: copy the MIT Kate Config definition | LOCAL | 0028 |
| PKT-0305 | `kate-syntax`: copy the MIT KDL definition | LOCAL | 0028 |
| PKT-0306 | `kate-syntax`: copy the MIT Klipper Config definition | LOCAL | 0028 |

## Wave PKT-0307

This is MIT copy wave 8 of 15.
Eight syntax files are copied from KSyntaxHighlighting `v6.31.0`.
52 MIT files remain queued.
A Cursor local agent on the Framework machine copies each file.
The owner Mac stays out of this wave.
The wiring packets are PKT-0429 through PKT-0436.
Packet numbers 0237 through 0244 stay unused.
The index is `docs/design/WAVE-0307.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0307 | `kate-syntax`: copy the MIT Klipper G-Code definition | LOCAL | 0028 |
| PKT-0308 | `kate-syntax`: copy the MIT LaTeX Log File definition | LOCAL | 0028 |
| PKT-0309 | `kate-syntax`: copy the MIT Log File (advanced) Selector definition | LOCAL | 0028 |
| PKT-0310 | `kate-syntax`: copy the MIT Log File (simplified) Selector definition | LOCAL | 0028 |
| PKT-0311 | `kate-syntax`: copy the MIT Log File (simplified) definition | LOCAL | 0028 |
| PKT-0312 | `kate-syntax`: copy the MIT MapCSS definition | LOCAL | 0028 |
| PKT-0313 | `kate-syntax`: copy the MIT Metamath definition | LOCAL | 0028 |
| PKT-0314 | `kate-syntax`: copy the MIT MIB definition | LOCAL | 0028 |

## Wave PKT-0315

This is MIT copy wave 9 of 15.
Eight syntax files are copied from KSyntaxHighlighting `v6.31.0`.
44 MIT files remain queued.
A Cursor local agent on the Framework machine copies each file.
The owner Mac stays out of this wave.
The wiring packets are PKT-0437 through PKT-0444.
Packet numbers 0237 through 0244 stay unused.
The index is `docs/design/WAVE-0315.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0315 | `kate-syntax`: copy the MIT MIPS Assembler definition | LOCAL | 0028 |
| PKT-0316 | `kate-syntax`: copy the MIT Modula-2 (ISO only) definition | LOCAL | 0028 |
| PKT-0317 | `kate-syntax`: copy the MIT Modula-2 (PIM only) definition | LOCAL | 0028 |
| PKT-0318 | `kate-syntax`: copy the MIT Modula-2 (R10 only) definition | LOCAL | 0028 |
| PKT-0319 | `kate-syntax`: copy the MIT Modula-2 definition | LOCAL | 0028 |
| PKT-0320 | `kate-syntax`: copy the MIT NFTables definition | LOCAL | 0028 |
| PKT-0321 | `kate-syntax`: copy the MIT nginx Configuration definition | LOCAL | 0028 |
| PKT-0322 | `kate-syntax`: copy the MIT Ninja definition | LOCAL | 0028 |

## Wave PKT-0323

This is MIT copy wave 10 of 15.
Eight syntax files are copied from KSyntaxHighlighting `v6.31.0`.
36 MIT files remain queued.
A Cursor local agent on the Framework machine copies each file.
The owner Mac stays out of this wave.
The wiring packets are PKT-0445 through PKT-0452.
Packet numbers 0237 through 0244 stay unused.
The index is `docs/design/WAVE-0323.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0323 | `kate-syntax`: copy the MIT Nix definition | LOCAL | 0028 |
| PKT-0324 | `kate-syntax`: copy the MIT Odin definition | LOCAL | 0028 |
| PKT-0325 | `kate-syntax`: copy the MIT OORS definition | LOCAL | 0028 |
| PKT-0326 | `kate-syntax`: copy the MIT OpenSCAD definition | LOCAL | 0028 |
| PKT-0327 | `kate-syntax`: copy the MIT Overpass QL definition | LOCAL | 0028 |
| PKT-0328 | `kate-syntax`: copy the MIT PIO Assembler definition | LOCAL | 0028 |
| PKT-0329 | `kate-syntax`: copy the MIT Pony definition | LOCAL | 0028 |
| PKT-0330 | `kate-syntax`: copy the MIT PowerShell definition | LOCAL | 0028 |

## Wave PKT-0331

This is MIT copy wave 11 of 15.
Eight syntax files are copied from KSyntaxHighlighting `v6.31.0`.
28 MIT files remain queued.
A Cursor local agent on the Framework machine copies each file.
The owner Mac stays out of this wave.
The wiring packets are PKT-0453 through PKT-0460.
Packet numbers 0237 through 0244 stay unused.
The index is `docs/design/WAVE-0331.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0331 | `kate-syntax`: copy the MIT QDoc Configuration definition | LOCAL | 0028 |
| PKT-0332 | `kate-syntax`: copy the MIT QFace definition | LOCAL | 0028 |
| PKT-0333 | `kate-syntax`: copy the MIT QMake definition | LOCAL | 0028 |
| PKT-0334 | `kate-syntax`: copy the MIT QML definition | LOCAL | 0028 |
| PKT-0335 | `kate-syntax`: copy the MIT R documentation definition | LOCAL | 0028 |
| PKT-0336 | `kate-syntax`: copy the MIT RenPy definition | LOCAL | 0028 |
| PKT-0337 | `kate-syntax`: copy the MIT RETRO definition | LOCAL | 0028 |
| PKT-0338 | `kate-syntax`: copy the MIT Robot definition | LOCAL | 0028 |

## Wave PKT-0339

This is MIT copy wave 12 of 15.
Eight syntax files are copied from KSyntaxHighlighting `v6.31.0`.
20 MIT files remain queued.
A Cursor local agent on the Framework machine copies each file.
The owner Mac stays out of this wave.
The wiring packets are PKT-0461 through PKT-0468.
Packet numbers 0237 through 0244 stay unused.
The index is `docs/design/WAVE-0339.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0339 | `kate-syntax`: copy the MIT SASS definition | LOCAL | 0028 |
| PKT-0340 | `kate-syntax`: copy the MIT Sieve definition | LOCAL | 0028 |
| PKT-0341 | `kate-syntax`: copy the MIT Slint definition | LOCAL | 0028 |
| PKT-0342 | `kate-syntax`: copy the MIT Smali definition | LOCAL | 0028 |
| PKT-0343 | `kate-syntax`: copy the MIT SML definition | LOCAL | 0028 |
| PKT-0344 | `kate-syntax`: copy the MIT Snort/Suricata definition | LOCAL | 0028 |
| PKT-0345 | `kate-syntax`: copy the MIT Solidity definition | LOCAL | 0028 |
| PKT-0346 | `kate-syntax`: copy the MIT SPARQL definition | LOCAL | 0028 |

## Wave PKT-0347

This is MIT copy wave 13 of 15.
Eight syntax files are copied from KSyntaxHighlighting `v6.31.0`.
12 MIT files remain queued.
A Cursor local agent on the Framework machine copies each file.
The owner Mac stays out of this wave.
The wiring packets are PKT-0469 through PKT-0476.
Packet numbers 0237 through 0244 stay unused.
The index is `docs/design/WAVE-0347.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0347 | `kate-syntax`: copy the MIT IATA SSIM definition | LOCAL | 0028 |
| PKT-0348 | `kate-syntax`: copy the MIT Stan definition | LOCAL | 0028 |
| PKT-0349 | `kate-syntax`: copy the MIT STEP definition | LOCAL | 0028 |
| PKT-0350 | `kate-syntax`: copy the MIT Swift definition | LOCAL | 0028 |
| PKT-0351 | `kate-syntax`: copy the MIT Terraform definition | LOCAL | 0028 |
| PKT-0352 | `kate-syntax`: copy the MIT TextProto definition | LOCAL | 0028 |
| PKT-0353 | `kate-syntax`: copy the MIT Tiger definition | LOCAL | 0028 |
| PKT-0354 | `kate-syntax`: copy the MIT Todo.txt definition | LOCAL | 0028 |

## Wave PKT-0355

This is MIT copy wave 14 of 15.
Eight syntax files are copied from KSyntaxHighlighting `v6.31.0`.
4 MIT files remain queued.
A Cursor local agent on the Framework machine copies each file.
The owner Mac stays out of this wave.
The wiring packets are PKT-0477 through PKT-0484.
Packet numbers 0237 through 0244 stay unused.
The index is `docs/design/WAVE-0355.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0355 | `kate-syntax`: copy the MIT toit definition | LOCAL | 0028 |
| PKT-0356 | `kate-syntax`: copy the MIT TSV definition | LOCAL | 0028 |
| PKT-0357 | `kate-syntax`: copy the MIT Twig/Twig definition | LOCAL | 0028 |
| PKT-0358 | `kate-syntax`: copy the MIT Typst definition | LOCAL | 0028 |
| PKT-0359 | `kate-syntax`: copy the MIT V definition | LOCAL | 0028 |
| PKT-0360 | `kate-syntax`: copy the MIT Viper definition | LOCAL | 0028 |
| PKT-0361 | `kate-syntax`: copy the MIT Wayland Trace definition | LOCAL | 0028 |
| PKT-0362 | `kate-syntax`: copy the MIT Web Video Text Tracks definition | LOCAL | 0028 |

## Wave PKT-0363

This is MIT copy wave 15 of 15.
Four syntax files are copied from KSyntaxHighlighting `v6.31.0`.
No MIT file remains queued.
A Cursor local agent on the Framework machine copies each file.
The owner Mac stays out of this wave.
The wiring packets are PKT-0485 through PKT-0488.
Packet numbers 0237 through 0244 stay unused.
The index is `docs/design/WAVE-0363.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0363 | `kate-syntax`: copy the MIT XHTML definition | LOCAL | 0028 |
| PKT-0364 | `kate-syntax`: copy the MIT XKeyboardConfig definition | LOCAL | 0028 |
| PKT-0365 | `kate-syntax`: copy the MIT YARA definition | LOCAL | 0028 |
| PKT-0366 | `kate-syntax`: copy the MIT Zig definition | LOCAL | 0028 |

## Wave PKT-0367

This is include_str wiring wave 1 of 15.
It wires the eight files of MIT copy wave 1.
A Cursor local agent on the Framework machine codes each packet.
The wiring packets share `vendored.rs` and `SOURCES.md`, so they run one after the other.
PKT-0367 moves the vendored list to `vendored.rs`.
PKT-0368 to PKT-0371 fix the engine for `cabal.xml`, `d2.xml`, `fortran-free.xml`, `sass.xml`, `doxygen.xml`, `ninja.xml`, `rdoc.xml`, `mib.xml`, and `nginx.xml`.
PKT-0372 is the one Mac packet. It makes the window load every vendored definition.
The index is `docs/design/WAVE-0367.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0367 | `kate-syntax`: list every vendored definition in vendored.rs | LOCAL | 0028, 0231 |
| PKT-0368 | `kate-syntax`: skip a keyword include from a missing definition | LOCAL | 0367 |
| PKT-0369 | `kate-syntax`: escape a literal bracket inside a character class | LOCAL | 0228 |
| PKT-0370 | `kate-syntax`: read a branch reset group | LOCAL | 0369 |
| PKT-0371 | `kate-syntax`: accept white space between ] and > in a doctype | LOCAL | 0007 |
| PKT-0372 | `kate-ui`: load every vendored definition at launch | LOCAL | 0367 |
| PKT-0373 | `kate-syntax`: wire the ABNF definition into VENDORED | LOCAL | 0251, 0367 |
| PKT-0374 | `kate-syntax`: wire the Adblock Plus definition into VENDORED | LOCAL | 0252, 0373 |
| PKT-0375 | `kate-syntax`: wire the AHDL definition into VENDORED | LOCAL | 0253, 0374 |
| PKT-0376 | `kate-syntax`: wire the ANTLR definition into VENDORED | LOCAL | 0254, 0375 |
| PKT-0377 | `kate-syntax`: wire the ARM Assembler definition into VENDORED | LOCAL | 0255, 0376 |
| PKT-0378 | `kate-syntax`: wire the BrightScript definition into VENDORED | LOCAL | 0256, 0377 |
| PKT-0379 | `kate-syntax`: wire the Cabal definition into VENDORED | LOCAL | 0257, 0378, 0368 |
| PKT-0380 | `kate-syntax`: wire the Cap'n Proto definition into VENDORED | LOCAL | 0258, 0379 |

## Wave PKT-0381

This is include_str wiring wave 2 of 15.
It wires the eight files of MIT copy wave 2.
A Cursor local agent on the Framework machine codes each packet.
The wiring packets share `vendored.rs` and `SOURCES.md`, so they run one after the other.
The index is `docs/design/WAVE-0381.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0381 | `kate-syntax`: wire the CashScript definition into VENDORED | LOCAL | 0259, 0380 |
| PKT-0382 | `kate-syntax`: wire the ChangeLog definition into VENDORED | LOCAL | 0260, 0381 |
| PKT-0383 | `kate-syntax`: wire the Common Intermediate Language (CIL) definition into VENDORED | LOCAL | 0261, 0382 |
| PKT-0384 | `kate-syntax`: wire the CLIST definition into VENDORED | LOCAL | 0262, 0383 |
| PKT-0385 | `kate-syntax`: wire the COBOL definition into VENDORED | LOCAL | 0263, 0384 |
| PKT-0386 | `kate-syntax`: wire the CoffeeScript definition into VENDORED | LOCAL | 0264, 0385 |
| PKT-0387 | `kate-syntax`: wire the Common Lisp definition into VENDORED | LOCAL | 0265, 0386 |
| PKT-0388 | `kate-syntax`: wire the CSV (pipe) definition into VENDORED | LOCAL | 0266, 0387 |

## Wave PKT-0389

This is include_str wiring wave 3 of 15.
It wires the eight files of MIT copy wave 3.
A Cursor local agent on the Framework machine codes each packet.
The wiring packets share `vendored.rs` and `SOURCES.md`, so they run one after the other.
The index is `docs/design/WAVE-0389.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0389 | `kate-syntax`: wire the CSV (semicolon) definition into VENDORED | LOCAL | 0267, 0388 |
| PKT-0390 | `kate-syntax`: wire the CSV (whitespace) definition into VENDORED | LOCAL | 0268, 0389 |
| PKT-0391 | `kate-syntax`: wire the D2 definition into VENDORED | LOCAL | 0269, 0390, 0368 |
| PKT-0392 | `kate-syntax`: wire the Dart definition into VENDORED | LOCAL | 0270, 0391 |
| PKT-0393 | `kate-syntax`: wire the Dockerfile definition into VENDORED | LOCAL | 0271, 0392 |
| PKT-0394 | `kate-syntax`: wire the DotEnv definition into VENDORED | LOCAL | 0272, 0393 |
| PKT-0395 | `kate-syntax`: wire the Doxygen definition into VENDORED | LOCAL | 0273, 0394, 0369 |
| PKT-0396 | `kate-syntax`: wire the Earthfile definition into VENDORED | LOCAL | 0274, 0395 |

## Wave PKT-0397

This is include_str wiring wave 4 of 15.
It wires the eight files of MIT copy wave 4.
A Cursor local agent on the Framework machine codes each packet.
The wiring packets share `vendored.rs` and `SOURCES.md`, so they run one after the other.
The index is `docs/design/WAVE-0397.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0397 | `kate-syntax`: wire the Elixir/EEx definition into VENDORED | LOCAL | 0275, 0396 |
| PKT-0398 | `kate-syntax`: wire the Elixir/HEEx definition into VENDORED | LOCAL | 0276, 0397 |
| PKT-0399 | `kate-syntax`: wire the Elm definition into VENDORED | LOCAL | 0277, 0398 |
| PKT-0400 | `kate-syntax`: wire the Email definition into VENDORED | LOCAL | 0278, 0399 |
| PKT-0401 | `kate-syntax`: wire the Expect definition into VENDORED | LOCAL | 0279, 0400 |
| PKT-0402 | `kate-syntax`: wire the FlatBuffers definition into VENDORED | LOCAL | 0280, 0401 |
| PKT-0403 | `kate-syntax`: wire the Fluent definition into VENDORED | LOCAL | 0281, 0402 |
| PKT-0404 | `kate-syntax`: wire the Fortran (Free Format) definition into VENDORED | LOCAL | 0282, 0403, 0368 |

## Wave PKT-0405

This is include_str wiring wave 5 of 15.
It wires the eight files of MIT copy wave 5.
A Cursor local agent on the Framework machine codes each packet.
The wiring packets share `vendored.rs` and `SOURCES.md`, so they run one after the other.
The index is `docs/design/WAVE-0405.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0405 | `kate-syntax`: wire the GDL definition into VENDORED | LOCAL | 0283, 0404 |
| PKT-0406 | `kate-syntax`: wire the Gemtext definition into VENDORED | LOCAL | 0284, 0405 |
| PKT-0407 | `kate-syntax`: wire the GNU Gettext definition into VENDORED | LOCAL | 0285, 0406 |
| PKT-0408 | `kate-syntax`: wire the Cucumber Gherkin feature definition into VENDORED | LOCAL | 0286, 0407 |
| PKT-0409 | `kate-syntax`: wire the Gleam definition into VENDORED | LOCAL | 0287, 0408 |
| PKT-0410 | `kate-syntax`: wire the GN definition into VENDORED | LOCAL | 0288, 0409 |
| PKT-0411 | `kate-syntax`: wire the GPRBuild definition into VENDORED | LOCAL | 0289, 0410 |
| PKT-0412 | `kate-syntax`: wire the GraphQL definition into VENDORED | LOCAL | 0290, 0411 |

## Wave PKT-0413

This is include_str wiring wave 6 of 15.
It wires the eight files of MIT copy wave 6.
A Cursor local agent on the Framework machine codes each packet.
The wiring packets share `vendored.rs` and `SOURCES.md`, so they run one after the other.
The index is `docs/design/WAVE-0413.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0413 | `kate-syntax`: wire the GTK Blueprint definition into VENDORED | LOCAL | 0291, 0412 |
| PKT-0414 | `kate-syntax`: wire the Hare definition into VENDORED | LOCAL | 0292, 0413 |
| PKT-0415 | `kate-syntax`: wire the Haxe definition into VENDORED | LOCAL | 0293, 0414 |
| PKT-0416 | `kate-syntax`: wire the Hjson definition into VENDORED | LOCAL | 0294, 0415 |
| PKT-0417 | `kate-syntax`: wire the InnoSetup definition into VENDORED | LOCAL | 0295, 0416 |
| PKT-0418 | `kate-syntax`: wire the Intel HEX definition into VENDORED | LOCAL | 0296, 0417 |
| PKT-0419 | `kate-syntax`: wire the Java Module definition into VENDORED | LOCAL | 0297, 0418 |
| PKT-0420 | `kate-syntax`: wire the Java Properties definition into VENDORED | LOCAL | 0298, 0419 |

## Wave PKT-0421

This is include_str wiring wave 7 of 15.
It wires the eight files of MIT copy wave 7.
A Cursor local agent on the Framework machine codes each packet.
The wiring packets share `vendored.rs` and `SOURCES.md`, so they run one after the other.
The index is `docs/design/WAVE-0421.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0421 | `kate-syntax`: wire the JCL definition into VENDORED | LOCAL | 0299, 0420 |
| PKT-0422 | `kate-syntax`: wire the Jinja definition into VENDORED | LOCAL | 0300, 0421 |
| PKT-0423 | `kate-syntax`: wire the JSON5 definition into VENDORED | LOCAL | 0301, 0422 |
| PKT-0424 | `kate-syntax`: wire the Jsonnet definition into VENDORED | LOCAL | 0302, 0423 |
| PKT-0425 | `kate-syntax`: wire the Just definition into VENDORED | LOCAL | 0303, 0424 |
| PKT-0426 | `kate-syntax`: wire the Kate Config definition into VENDORED | LOCAL | 0304, 0425 |
| PKT-0427 | `kate-syntax`: wire the KDL definition into VENDORED | LOCAL | 0305, 0426 |
| PKT-0428 | `kate-syntax`: wire the Klipper Config definition into VENDORED | LOCAL | 0306, 0427 |

## Wave PKT-0429

This is include_str wiring wave 8 of 15.
It wires the eight files of MIT copy wave 8.
A Cursor local agent on the Framework machine codes each packet.
The wiring packets share `vendored.rs` and `SOURCES.md`, so they run one after the other.
The index is `docs/design/WAVE-0429.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0429 | `kate-syntax`: wire the Klipper G-Code definition into VENDORED | LOCAL | 0307, 0428 |
| PKT-0430 | `kate-syntax`: wire the LaTeX Log File definition into VENDORED | LOCAL | 0308, 0429 |
| PKT-0431 | `kate-syntax`: wire the Log File (advanced) Selector definition into VENDORED | LOCAL | 0309, 0430 |
| PKT-0432 | `kate-syntax`: wire the Log File (simplified) Selector definition into VENDORED | LOCAL | 0310, 0431 |
| PKT-0433 | `kate-syntax`: wire the Log File (simplified) definition into VENDORED | LOCAL | 0311, 0432 |
| PKT-0434 | `kate-syntax`: wire the MapCSS definition into VENDORED | LOCAL | 0312, 0433 |
| PKT-0435 | `kate-syntax`: wire the Metamath definition into VENDORED | LOCAL | 0313, 0434 |
| PKT-0436 | `kate-syntax`: wire the MIB definition into VENDORED | LOCAL | 0314, 0435, 0370 |

## Wave PKT-0437

This is include_str wiring wave 9 of 15.
It wires the eight files of MIT copy wave 9.
A Cursor local agent on the Framework machine codes each packet.
The wiring packets share `vendored.rs` and `SOURCES.md`, so they run one after the other.
The index is `docs/design/WAVE-0437.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0437 | `kate-syntax`: wire the MIPS Assembler definition into VENDORED | LOCAL | 0315, 0436 |
| PKT-0438 | `kate-syntax`: wire the Modula-2 (ISO only) definition into VENDORED | LOCAL | 0316, 0437 |
| PKT-0439 | `kate-syntax`: wire the Modula-2 (PIM only) definition into VENDORED | LOCAL | 0317, 0438 |
| PKT-0440 | `kate-syntax`: wire the Modula-2 (R10 only) definition into VENDORED | LOCAL | 0318, 0439 |
| PKT-0441 | `kate-syntax`: wire the Modula-2 definition into VENDORED | LOCAL | 0319, 0440 |
| PKT-0442 | `kate-syntax`: wire the NFTables definition into VENDORED | LOCAL | 0320, 0441 |
| PKT-0443 | `kate-syntax`: wire the nginx Configuration definition into VENDORED | LOCAL | 0321, 0442, 0371 |
| PKT-0444 | `kate-syntax`: wire the Ninja definition into VENDORED | LOCAL | 0322, 0443, 0369 |

## Wave PKT-0445

This is include_str wiring wave 10 of 15.
It wires the eight files of MIT copy wave 10.
A Cursor local agent on the Framework machine codes each packet.
The wiring packets share `vendored.rs` and `SOURCES.md`, so they run one after the other.
The index is `docs/design/WAVE-0445.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0445 | `kate-syntax`: wire the Nix definition into VENDORED | LOCAL | 0323, 0444 |
| PKT-0446 | `kate-syntax`: wire the Odin definition into VENDORED | LOCAL | 0324, 0445 |
| PKT-0447 | `kate-syntax`: wire the OORS definition into VENDORED | LOCAL | 0325, 0446 |
| PKT-0448 | `kate-syntax`: wire the OpenSCAD definition into VENDORED | LOCAL | 0326, 0447 |
| PKT-0449 | `kate-syntax`: wire the Overpass QL definition into VENDORED | LOCAL | 0327, 0448 |
| PKT-0450 | `kate-syntax`: wire the PIO Assembler definition into VENDORED | LOCAL | 0328, 0449 |
| PKT-0451 | `kate-syntax`: wire the Pony definition into VENDORED | LOCAL | 0329, 0450 |
| PKT-0452 | `kate-syntax`: wire the PowerShell definition into VENDORED | LOCAL | 0330, 0451 |

## Wave PKT-0453

This is include_str wiring wave 11 of 15.
It wires the eight files of MIT copy wave 11.
A Cursor local agent on the Framework machine codes each packet.
The wiring packets share `vendored.rs` and `SOURCES.md`, so they run one after the other.
The index is `docs/design/WAVE-0453.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0453 | `kate-syntax`: wire the QDoc Configuration definition into VENDORED | LOCAL | 0331, 0452 |
| PKT-0454 | `kate-syntax`: wire the QFace definition into VENDORED | LOCAL | 0332, 0453 |
| PKT-0455 | `kate-syntax`: wire the QMake definition into VENDORED | LOCAL | 0333, 0454 |
| PKT-0456 | `kate-syntax`: wire the QML definition into VENDORED | LOCAL | 0334, 0455 |
| PKT-0457 | `kate-syntax`: wire the R documentation definition into VENDORED | LOCAL | 0335, 0456, 0369 |
| PKT-0458 | `kate-syntax`: wire the RenPy definition into VENDORED | LOCAL | 0336, 0457 |
| PKT-0459 | `kate-syntax`: wire the RETRO definition into VENDORED | LOCAL | 0337, 0458 |
| PKT-0460 | `kate-syntax`: wire the Robot definition into VENDORED | LOCAL | 0338, 0459 |

## Wave PKT-0461

This is include_str wiring wave 12 of 15.
It wires the eight files of MIT copy wave 12.
A Cursor local agent on the Framework machine codes each packet.
The wiring packets share `vendored.rs` and `SOURCES.md`, so they run one after the other.
The index is `docs/design/WAVE-0461.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0461 | `kate-syntax`: wire the SASS definition into VENDORED | LOCAL | 0339, 0460, 0368 |
| PKT-0462 | `kate-syntax`: wire the Sieve definition into VENDORED | LOCAL | 0340, 0461 |
| PKT-0463 | `kate-syntax`: wire the Slint definition into VENDORED | LOCAL | 0341, 0462 |
| PKT-0464 | `kate-syntax`: wire the Smali definition into VENDORED | LOCAL | 0342, 0463 |
| PKT-0465 | `kate-syntax`: wire the SML definition into VENDORED | LOCAL | 0343, 0464 |
| PKT-0466 | `kate-syntax`: wire the Snort/Suricata definition into VENDORED | LOCAL | 0344, 0465 |
| PKT-0467 | `kate-syntax`: wire the Solidity definition into VENDORED | LOCAL | 0345, 0466 |
| PKT-0468 | `kate-syntax`: wire the SPARQL definition into VENDORED | LOCAL | 0346, 0467 |

## Wave PKT-0469

This is include_str wiring wave 13 of 15.
It wires the eight files of MIT copy wave 13.
A Cursor local agent on the Framework machine codes each packet.
The wiring packets share `vendored.rs` and `SOURCES.md`, so they run one after the other.
The index is `docs/design/WAVE-0469.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0469 | `kate-syntax`: wire the IATA SSIM definition into VENDORED | LOCAL | 0347, 0468 |
| PKT-0470 | `kate-syntax`: wire the Stan definition into VENDORED | LOCAL | 0348, 0469 |
| PKT-0471 | `kate-syntax`: wire the STEP definition into VENDORED | LOCAL | 0349, 0470 |
| PKT-0472 | `kate-syntax`: wire the Swift definition into VENDORED | LOCAL | 0350, 0471 |
| PKT-0473 | `kate-syntax`: wire the Terraform definition into VENDORED | LOCAL | 0351, 0472 |
| PKT-0474 | `kate-syntax`: wire the TextProto definition into VENDORED | LOCAL | 0352, 0473 |
| PKT-0475 | `kate-syntax`: wire the Tiger definition into VENDORED | LOCAL | 0353, 0474 |
| PKT-0476 | `kate-syntax`: wire the Todo.txt definition into VENDORED | LOCAL | 0354, 0475 |

## Wave PKT-0477

This is include_str wiring wave 14 of 15.
It wires the eight files of MIT copy wave 14.
A Cursor local agent on the Framework machine codes each packet.
The wiring packets share `vendored.rs` and `SOURCES.md`, so they run one after the other.
The index is `docs/design/WAVE-0477.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0477 | `kate-syntax`: wire the toit definition into VENDORED | LOCAL | 0355, 0476 |
| PKT-0478 | `kate-syntax`: wire the TSV definition into VENDORED | LOCAL | 0356, 0477 |
| PKT-0479 | `kate-syntax`: wire the Twig/Twig definition into VENDORED | LOCAL | 0357, 0478 |
| PKT-0480 | `kate-syntax`: wire the Typst definition into VENDORED | LOCAL | 0358, 0479 |
| PKT-0481 | `kate-syntax`: wire the V definition into VENDORED | LOCAL | 0359, 0480 |
| PKT-0482 | `kate-syntax`: wire the Viper definition into VENDORED | LOCAL | 0360, 0481 |
| PKT-0483 | `kate-syntax`: wire the Wayland Trace definition into VENDORED | LOCAL | 0361, 0482 |
| PKT-0484 | `kate-syntax`: wire the Web Video Text Tracks definition into VENDORED | LOCAL | 0362, 0483 |

## Wave PKT-0485

This is include_str wiring wave 15 of 15.
It wires the four files of MIT copy wave 15.
A Cursor local agent on the Framework machine codes each packet.
The wiring packets share `vendored.rs` and `SOURCES.md`, so they run one after the other.
The index is `docs/design/WAVE-0485.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0485 | `kate-syntax`: wire the XHTML definition into VENDORED | LOCAL | 0363, 0484 |
| PKT-0486 | `kate-syntax`: wire the XKeyboardConfig definition into VENDORED | LOCAL | 0364, 0485 |
| PKT-0487 | `kate-syntax`: wire the YARA definition into VENDORED | LOCAL | 0365, 0486 |
| PKT-0488 | `kate-syntax`: wire the Zig definition into VENDORED | LOCAL | 0366, 0487 |

## Wave PKT-0489

This wave finishes Phase 2.
The model slices of WAVE-0020 and WAVE-0024 are on main, but the window does not call them yet.
Each packet here either adds a command to `kate-core` or makes the window use a slice.
The CLOUD packets edit `command.rs`, `workspace.rs`, and `view.rs` and run one after the other.
The LOCAL packets edit `kate-ui` and run on the owner Mac.
A `kate-ui` packet that paints lists `GUI_DISPLAY` and `OWNER_EYES`; the owner looks at the window after it.
A `kate-ui` packet that only adds a model file lists `MACOS_HOST`.
Phase 2 ends when this wave, the fifteen copy waves, and the fifteen wiring waves are on main.
The index is `docs/design/WAVE-0489.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0489 | kate-core: compare the file on disk with the last read | CLOUD | — |
| PKT-0490 | kate-core: select the next copy of the primary selection | CLOUD | 0489 |
| PKT-0491 | kate-core: replace the selections of a view | CLOUD | 0489, 0490 |
| PKT-0492 | kate-core: split, activate, and list the views from the workspace | CLOUD | 0489, 0490, 0491 |
| PKT-0493 | kate-core: fold and unfold the region at the primary caret | CLOUD | 0489, 0490, 0491, 0492 |
| PKT-0494 | kate-core: replace a list of ranges in one transaction | CLOUD | 0489, 0490, 0491, 0492, 0493 |
| PKT-0495 | kate-core: list the lines edited since the last save | CLOUD | 0489, 0490, 0491, 0492, 0493, 0494 |
| PKT-0496 | kate-ui: map visual rows to lines around collapsed folds | LOCAL | — |
| PKT-0497 | kate-ui: build the tab rows | LOCAL | 0492 |
| PKT-0498 | kate-ui: paint the tab bar and switch tabs | LOCAL | 0492, 0497 |
| PKT-0499 | kate-ui: split the window after the active view | LOCAL | 0498 |
| PKT-0500 | kate-ui: find bar state over kate-search | LOCAL | 0491, 0494, 0497 |
| PKT-0501 | kate-ui: show the find bar | LOCAL | 0498, 0499, 0500 |
| PKT-0502 | kate-ui: bind cmd-d and cmd-alt-bracket | LOCAL | 0490, 0493 |
| PKT-0503 | kate-ui: place carets with the mouse | LOCAL | 0491, 0498, 0499, 0501, 0502 |
| PKT-0504 | kate-ui: name the session file and write it atomically | LOCAL | — |
| PKT-0505 | kate-ui: save the session on quit and restore it on launch | LOCAL | 0498, 0499, 0501, 0503, 0504 |
| PKT-0506 | kate-ui: watch the open files and ask about a disk change | LOCAL | 0489, 0498, 0499, 0500, 0501, 0503, 0505 |
| PKT-0507 | kate-ui: follow the system theme | LOCAL | 0498, 0499, 0501, 0503, 0505, 0506 |
| PKT-0508 | kate-ui: paint fold markers, bookmarks, and the modified bar in the gutter | LOCAL | 0493, 0495, 0498, 0499, 0501, 0502, 0503, 0505, 0506, 0507 |
| PKT-0509 | kate-ui: hide the lines of a collapsed fold | LOCAL | 0496, 0498, 0499, 0501, 0502, 0503, 0505, 0506, 0507, 0508 |
| PKT-0510 | kate-ui: wrap long lines and move by visual rows | LOCAL | 0496, 0498, 0499, 0501, 0503, 0505, 0506, 0507, 0508, 0509 |
| PKT-0511 | kate-ui: read the user config at launch | LOCAL | 0498, 0499, 0501, 0503, 0505, 0506, 0507, 0508, 0509, 0510 |

## Wave PKT-0030

This wave is the first Phase 3 wave.
It expands the title PKT-0030 from the backlog.
A new crate `kate-session` reads `.kateproject` files the way Kate's project plugin does.
The crate creation packet runs in the cloud and leaves empty module files for the three Mac packets, so those can run together.
`Project::files` joins the sources and applies the exclude patterns.
`find_project_file` lets the window find the project of an open file.
The session JSON stays in `kate-ui/src/profile.rs`; this crate holds projects only.
svn, hg, darcs, fossil, and nested `projects` entries are not read in this version.
The index is `docs/design/WAVE-0030.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0030 | kate-session: create the crate and read a .kateproject file | CLOUD | 0500, 0506, 0512 |
| PKT-0512 | gatecheck: map kate-session and kate-lsp to LGPL-2.1-or-later | EITHER | — |
| PKT-0513 | kate-session: list the files of a directory source | LOCAL | 0030 |
| PKT-0514 | kate-session: list the files that git tracks | LOCAL | 0030 |
| PKT-0515 | kate-session: drop the paths that match an exclude pattern | LOCAL | 0030 |
| PKT-0516 | kate-session: collect the project files | LOCAL | 0030, 0513, 0514, 0515 |
| PKT-0517 | kate-session: find and load the project file for a path | LOCAL | 0030, 0516 |

## Wave PKT-0031

This wave expands the titles PKT-0031 and PKT-0032.
The tree, the document list, and the palette rows are model files in `kate-ui` with no GPUI import.
The two paint packets wait for the tab and split packets of WAVE-0489.
`fuzzy_rank` and `fuzzy_match_path` extend the fuzzy matcher that the earlier LOCAL waves built; this wave does not port `kfts_fuzzy_match.h` again.
`kate-ui` gains the `kate-session` dependency in the packet that loads the project of the open file.
The index is `docs/design/WAVE-0031.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0031 | kate-ui: build the project tree from a file list | LOCAL | 0497, 0500 |
| PKT-0032 | kate-core: rank candidates by fuzzy score | LOCAL | — |
| PKT-0518 | kate-ui: list the visible tree rows | LOCAL | 0031 |
| PKT-0519 | kate-ui: list the open documents | LOCAL | 0031, 0497, 0500 |
| PKT-0520 | kate-ui: load the project of the open file | LOCAL | 0030, 0498, 0499, 0500, 0501, 0503, 0505, 0506, 0507, 0508, 0509, 0510, 0511, 0517 |
| PKT-0521 | kate-ui: show the project tree and the document list panes | LOCAL | 0031, 0498, 0499, 0501, 0503, 0505, 0506, 0507, 0508, 0509, 0510, 0511, 0518, 0519, 0520 |
| PKT-0522 | kate-core: score a path by its file name first | LOCAL | 0032 |
| PKT-0523 | kate-ui: rank the quick open rows | LOCAL | 0031, 0497, 0500, 0519, 0522 |
| PKT-0524 | kate-ui: list the command palette entries | LOCAL | 0503, 0523 |
| PKT-0525 | kate-ui: open the quick open and command palettes | LOCAL | 0498, 0499, 0501, 0503, 0505, 0506, 0507, 0508, 0509, 0510, 0511, 0520, 0521, 0523, 0524 |

## Wave PKT-0033

This wave expands the title PKT-0033.
A new crate `kate-lsp` reads Kate's lspclient `settings.json`, frames JSON-RPC, converts positions, and turns a transaction into didChange events.
The fake server binary gives the client tests a real process over pipes with no network.
`kate-text` re-exports `Rope` so that `kate-core` and `kate-lsp` can name the snapshot type.
The outbox packet in `kate-core` hands every applied transaction to the shell with its pre-state text.
The parse and build helpers are small and marked EITHER; the sync builder is CLOUD.
The index is `docs/design/WAVE-0033.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0033 | kate-lsp: create the crate and read the servers settings | CLOUD | 0030, 0500, 0506, 0512, 0520, 0526 |
| PKT-0526 | kate-text: re-export Rope | EITHER | — |
| PKT-0527 | kate-lsp: pick the server for a highlighting mode | EITHER | 0033 |
| PKT-0528 | kate-lsp: frame a JSON-RPC message | EITHER | 0033 |
| PKT-0529 | kate-lsp: tell a request from a response and a notification | EITHER | 0033, 0528 |
| PKT-0530 | kate-lsp: convert positions between offsets and LSP positions | EITHER | 0033 |
| PKT-0531 | kate-lsp: turn a transaction into didChange events | CLOUD | 0033, 0530 |
| PKT-0532 | kate-lsp: add a fake language server for the tests | CLOUD | 0033, 0529 |
| PKT-0533 | kate-core: keep the applied transactions for the shell | CLOUD | 0489, 0490, 0491, 0492, 0493, 0494, 0495, 0522, 0526 |

## Wave PKT-0034

This wave expands the title PKT-0034.
`LspClient` is two threads and two channels: a reader thread fills a channel, the window drains it with `poll`.
Every test spawns the fake server through `CARGO_BIN_EXE_fake-lsp-server`.
Diagnostics are anchored in the buffer so they move with later edits.
Semantic tokens, inlay hints, code actions, rename, and formatting stay in Phase 4.
The index is `docs/design/WAVE-0034.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0034 | kate-lsp: start, initialize, and stop a server | CLOUD | 0033, 0527, 0532 |
| PKT-0534 | kate-lsp: poll the server for events | CLOUD | 0033, 0034 |
| PKT-0535 | kate-lsp: open, change, and close a document on the server | CLOUD | 0033, 0034, 0534 |
| PKT-0536 | kate-lsp: anchor the diagnostics in the buffer | EITHER | 0033, 0530 |
| PKT-0537 | kate-lsp: read a hover result | EITHER | 0033 |

## Wave PKT-0035

This wave expands the titles PKT-0035, PKT-0036, and PKT-0037.
`kate-ui` gains the `kate-lsp` dependency in the diagnostics model packet.
The window starts one server per server name, sends didOpen, didChange from `drain_changes`, and didClose, and polls on a timer.
Completion merges the buffer words from `word_completions` with the server items.
Each paint packet has a small pure helper with a unit test; the owner checks the rest in the window.
The index is `docs/design/WAVE-0035.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0035 | kate-ui: build the diagnostics rows and the underline spans | LOCAL | 0030, 0031, 0033, 0497, 0500, 0506, 0519, 0520, 0523, 0536 |
| PKT-0036 | kate-ui: keep the completion popup state | LOCAL | 0031, 0035, 0497, 0500, 0519, 0523 |
| PKT-0037 | kate-lsp: build and read completion requests | EITHER | 0033 |
| PKT-0538 | kate-ui: start a language server for the open document | LOCAL | 0035, 0498, 0499, 0501, 0503, 0505, 0506, 0507, 0508, 0509, 0510, 0511, 0520, 0521, 0525, 0531, 0533, 0535 |
| PKT-0539 | kate-ui: underline diagnostics, list them, and show hover | LOCAL | 0035, 0498, 0499, 0501, 0503, 0505, 0506, 0507, 0508, 0509, 0510, 0511, 0520, 0521, 0525, 0537, 0538 |
| PKT-0540 | kate-ui: show the completion popup | LOCAL | 0036, 0498, 0499, 0501, 0503, 0505, 0506, 0507, 0508, 0509, 0510, 0511, 0520, 0521, 0525, 0538, 0539 |
| PKT-0541 | kate-lsp: build and read definition and references requests | EITHER | 0033, 0037 |
| PKT-0542 | kate-ui: go to the definition and list the references | LOCAL | 0498, 0499, 0501, 0503, 0505, 0506, 0507, 0508, 0509, 0510, 0511, 0520, 0521, 0525, 0538, 0539, 0540, 0541 |

## Wave PKT-0038

This wave expands the title PKT-0038.
The journal is JSON Lines: one header line with the path, then one line per applied transaction.
The journal file name is the hex digest of the file bytes at the last read, so a changed file on disk voids the journal.
`kate-core` gains serde and serde_json for the journal.
Untitled modified documents are stashed at quit and reopened at launch.
The index is `docs/design/WAVE-0038.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0038 | kate-core: write a history entry as a journal line | CLOUD | 0030, 0033, 0035, 0500, 0506, 0520, 0522, 0533 |
| PKT-0543 | kate-core: append to, read, and clear a journal file | EITHER | 0038 |
| PKT-0544 | kate-core: replay a journal onto the text on disk | CLOUD | 0038, 0543 |
| PKT-0545 | kate-core: journal each applied transaction and clear it on save | CLOUD | 0489, 0490, 0491, 0492, 0493, 0494, 0495, 0533, 0544 |
| PKT-0546 | kate-core: list the journals that are left and stash untitled text | EITHER | 0038, 0522, 0533, 0543, 0544, 0545 |
| PKT-0547 | kate-ui: offer recovery at launch and stash unsaved text on quit | LOCAL | 0498, 0499, 0501, 0503, 0505, 0506, 0507, 0508, 0509, 0510, 0511, 0520, 0521, 0525, 0538, 0539, 0540, 0542, 0546 |

## Wave PKT-0039

This wave expands the titles PKT-0039 and PKT-0040.
`kate-search` reads files from disk and reuses `find_all` over a buffer per file.
The pane runs the search on a background thread and lists the hits by file.
The two indenters follow the rule text in each packet, not the upstream JavaScript, because `xml.js` is LGPL-2.0-only.
Python stays as `python_newline` already is.
The index is `docs/design/WAVE-0039.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0039 | kate-search: search one file on disk | LOCAL | — |
| PKT-0040 | kate-core: indent a CMake line after a block opener | EITHER | — |
| PKT-0548 | kate-search: search a list of files | LOCAL | 0039 |
| PKT-0549 | kate-ui: group the search hits by file | LOCAL | 0031, 0035, 0036, 0497, 0500, 0519, 0523, 0548 |
| PKT-0550 | kate-ui: show the search in files pane | LOCAL | 0498, 0499, 0501, 0503, 0505, 0506, 0507, 0508, 0509, 0510, 0511, 0520, 0521, 0525, 0538, 0539, 0540, 0542, 0547, 0549 |
| PKT-0551 | kate-core: indent an XML line after an opening tag | EITHER | 0040 |
| PKT-0552 | kate-core: choose the cmake and xml newline from the indenter name | EITHER | 0040, 0551 |

## Wave PKT-0041

This wave expands the titles PKT-0041 and PKT-0042.
The settings window edits the six `EditorConfig` fields through a TOML round trip, so the parser in `kate-core` stays the one source of validation.
The bundle packet adds `Info.plist`, `bundle.sh`, and a plist test; the icon, signing, and notarization are owner steps outside the repository.
The governance packet adds the `OWNER_LINUX` host kind so that a Gate 2 payload from the Framework machine can be honest.
The gatekeeper holds that packet because `docs/governance/` is a protected path.
The index is `docs/design/WAVE-0041.md`.

| Packet | Title | Lane | Depends on |
|---|---|---|---|
| PKT-0041 | kate-ui: list the settings fields and apply one edit | LOCAL | 0031, 0035, 0036, 0497, 0500, 0519, 0523, 0549 |
| PKT-0042 | katem5rust: add the app bundle files | LOCAL | — |
| PKT-0553 | kate-ui: show the settings window and save the config | LOCAL | 0041, 0498, 0499, 0501, 0503, 0505, 0506, 0507, 0508, 0509, 0510, 0511, 0520, 0521, 0525, 0538, 0539, 0540, 0542, 0547, 0550 |
| PKT-0554 | kate-ui: open the files that macOS sends | LOCAL | 0042, 0498, 0499, 0501, 0503, 0505, 0506, 0507, 0508, 0509, 0510, 0511, 0520, 0521, 0525, 0538, 0539, 0540, 0542, 0547, 0550, 0553 |
| PKT-0555 | governance: add the OWNER_LINUX host kind | CLOUD | — |

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
