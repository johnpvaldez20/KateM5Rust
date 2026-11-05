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
