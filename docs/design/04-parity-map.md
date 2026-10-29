# Feature parity map: Kate → KateM5Rust

Status legend: **PORT** (same behavior, our types), **ADAPT** (same user-visible
result, different mechanism), **DEFER** (planned, Phase given), **CUT** (not
in v1; may never be). Line counts are `.cpp` + `.h` in the pinned upstream
tags and are there to make the cost of each row visible.

## KTextEditor (`upstream/ktexteditor/src`, ~111k lines)

| Kate subsystem | Lines | Rust home | Status | Notes |
|---|---|---|---|---|
| `buffer/` TextBuffer, TextBlock, TextLine | 6.8k | `kate-text` | **ADAPT** | Rope instead of block/line arrays; `\n`-only; per-line state moves to owners. §1 |
| `buffer/` TextCursor, TextRange, TextHistory | (incl.) | `kate-text::anchor` | **ADAPT** | One mechanism: `Anchor` + `EditLog`. §2 |
| `buffer/` TextFolding | 1.2k | `kate-core::fold` | **DEFER** P2 | `FoldRegion { start: Anchor, end: Anchor, folded }` per view; regions from syntax `FoldEvent`s and indentation. |
| `buffer/` TextLoader, save, digest | (incl.) | `kate-fileio` | **ADAPT** | No encoding prober in v1; unmappable chars are errors. §7 |
| `buffer/` SecureTextBuffer (privileged save via KAuth) | 0.2k | — | **CUT** | Linux/polkit mechanism. On macOS, `sudo` in a terminal. |
| `undo/` | 1.4k | `kate-text::history` | **ADAPT** | Two transactions per entry; depth-at-save modified tracking. §3 |
| `document/` KateDocument | 9.2k | `kate-core::document`, `workspace` | **ADAPT** | Split into owning tree + `Command`/`Event`. §8 |
| `document/` modelines (`kate:` variables), `.editorconfig` | (incl.) | `kate-core::modeline` | **DEFER** P2 | The owner's files carry `// kate: ...` lines; honoring them is userspace. `.editorconfig` via `editorconfig` crate, same packet. |
| `document/` auto-brace, auto-wrap, smart newline | (incl.) | `kate-core::edit_rules` | **DEFER** P2 | Each is a `Command` transformation; no scripting. |
| `view/` KateView, KateViewInternal, helpers | 19.3k | `kate-core::view` + `kate-ui::EditorElement` | **ADAPT** | Model/paint split. Most of these lines are Qt widget code. |
| `view/` multi-cursor (secondary cursors) | (incl.) | `kate-core::selection` | **PORT** | First-class; no primary/secondary distinction. §4 |
| `view/` icon border, line numbers, marks, folding markers | (incl.) | `kate-ui` | **DEFER** P2 | Painted from `Document::marks` and fold state. |
| `view/` dynamic word wrap | (incl.) | `kate-ui` layout | **DEFER** P2 | Layout owns visual lines; core stays logical. |
| `view/` minimap scrollbar | (incl.) | `kate-ui` | **DEFER** P3 | Cheap once shaped-line cache exists. |
| `render/` KateRenderer, layout cache, text layout | 3.4k | `kate-ui::ShapedLineCache` | **ADAPT** | GPUI/Core Text shaping; cache keyed by `(line, Revision)`. |
| `syntax/` KateHighlight, theme config widgets | 4.5k | `kate-syntax` + `kate-core::SyntaxCache` | **ADAPT** | Engine ported from KSyntaxHighlighting; per-line interned states. §5 |
| `search/` plain, regex, search bar model | 3.4k | `kate-search` | **PORT** | Three modes, whole-word flag, one-transaction replace. §6 |
| `search/` search bar widget | (incl.) | `kate-ui::find_bar` | **DEFER** P2 | Incremental, highlight-all, replace, regex toggle. |
| `vimode/` | 16.4k | `kate-vimode` | **DEFER** P4 | A `Command` producer with its own state machine. Large; its own phase. |
| `completion/` code completion widget + models | 6.3k | `kate-core::completion` + `kate-ui` | **DEFER** P3 | Word completion first; LSP completion after `kate-lsp`. |
| `script/` QJSEngine indenters, commands, libraries | 3.2k + JS | `kate-core::indent` | **ADAPT** | Indenters ported to Rust one per packet; `normal` + `cstyle` in P2. No JS engine. |
| `scripttester/` | 4.6k | — | **CUT** | Tooling for the JS engine we do not have. |
| `spellcheck/` (Sonnet) | 3.0k | — | **DEFER** P4 | `NSSpellChecker` through the GUI crate when it comes; not Sonnet. |
| `swapfile/` | 1.0k | `kate-core::recovery` | **DEFER** P3 | Journal is `HistoryEntry` JSONL; no second format. §7 |
| `inputmode/` normal / vi switch | 1.1k | `kate-core::View::input_mode` | **DEFER** P4 | An enum, with vi mode. |
| `mode/` file type detection, mode menu | 2.7k | `kate-syntax::Repository::for_file_name` + modelines | **ADAPT** | Extension/mime matching is already in the definitions. |
| `dialogs/` config dialogs | 2.6k | `kate-ui::settings` | **DEFER** P3 | TOML config file first; a settings UI after. |
| `printing/` | 1.9k | — | **CUT** | Print-to-PDF via the system dialog is a GUI-crate afternoon if anyone asks. |
| `variableeditor/` | 1.7k | — | **CUT** | UI for editing modelines; modelines themselves are honored. |
| `export/` HTML export | 0.4k | — | **CUT** | |
| `utils/` KateConfig, autoindent, bookmarks, template handler, word counter, … | 12.0k | spread | **ADAPT / DEFER** | Config → `EditorConfig` TOML (P2). Bookmarks → marks (P2). Template/snippet handler → P4. |

## Kate application (`upstream/kate/apps/lib`, ~37.7k lines)

| Kate subsystem | Rust home | Status | Notes |
|---|---|---|---|
| KateApp, KateMainWindow, KateViewManager, KateViewSpace, KateSplitter, KateTabBar | `kate-core::Workspace::layout` + `kate-ui` | **ADAPT** | `SplitTree` is the model; GPUI draws it. P2. |
| KateDocManager | `Workspace::documents` | **PORT** | Open-once by canonical path; refuse-close-when-modified. |
| Sessions (`session/`) | `kate-session` | **ADAPT** | Own JSON format; Kate INI not read. §9. P2 |
| Quick open (`quickopen/`, fuzzy match `kfts_fuzzy_match.h`) | `kate-ui::palette` + `kate-core::fuzzy` | **DEFER** P3 | The fuzzy matcher is 300 lines and MIT; port it. |
| Welcome view, recent files | `kate-ui` | **DEFER** P3 | macOS "Open Recent" menu comes free from AppKit/GPUI. |
| Output view, diagnostics view (`diagnostics/`) | `kate-ui` panes | **DEFER** P3 | With LSP. |
| Git integration (`gitprocess`, `commitfilesview`, `filehistorywidget`, `diff/`) | — | **CUT** v1 | Possibly P5 as `kate-git`; no v1 commitment. |
| Stash manager (unsaved buffers survive restart) | `kate-core::recovery` | **DEFER** P3 | Same journal as crash recovery. |
| URL bar / breadcrumbs, text hints, drawing utils | `kate-ui` | **DEFER / CUT** | Breadcrumbs P3; the rest as needed. |
| D-Bus adaptor, running-instance detection | — | **CUT** | macOS: single instance via `NSApplication` reopen events; `open -a`. |
| KWrite (single-document variant) | — | **CUT** | One app. |

## Addons (`upstream/kate/addons`, 36 plugins, ~100k lines)

| Addon | Lines | Status | Notes |
|---|---|---|---|
| `lspclient` | 11.7k | **DEFER** P3 → `kate-lsp` | Config-file compatible. §11 |
| `project` | 15.0k | **DEFER** P3 → `kate-session::Project` + `kate-ui` tree | Reads `.kateproject`. Build targets later. |
| `search` (search in files) | 6.1k | **DEFER** P3 | Reuse `kate-search` over files; results pane in `kate-ui`. |
| `filetree` | 4.9k | **DEFER** P3 | Document list pane. |
| `filebrowser` | 1.2k | **DEFER** P3 | |
| `konsole` (embedded terminal) | 1.1k | **DEFER** P4 | Needs a terminal emulator crate (`alacritty_terminal`) and a GPUI terminal view. Real work; not v1. |
| `snippets`, `template` | 5.0k | **DEFER** P4 | |
| `symbolviewer` | 3.2k | **DEFER** P4 | LSP document symbols make most of it redundant. |
| `tabswitcher`, `close-except-like`, `openlink`, `rainbow-parens`, `show-nonprintable`, `colorpicker`, `bookmarks` | ~4k | **DEFER** P3–P4 | Each is small; each is its own packet. |
| `format` (external formatters) | 1.5k | **DEFER** P4 | LSP formatting first. |
| `externaltools`, `textfilter`, `keyboardmacros` | 4.1k | **DEFER** P4 | |
| `git-blame`, `compiler-explorer`, `preview`, `kategpgplugin`, `backtracebrowser`, `gdbplugin`, `katebuild-plugin`, `kate-ctags`, `katesql`, `xmltools`, `xmlcheck`, `rbql`, `replicode`, `latexunicodecompletion`, `cmake-tools`, `eslint` | ~45k | **CUT** | Not part of "a text editor a Kate user recognizes". Any one can return as a packet with a user behind it. |

## What "v1" is

Phases 0–3 of `05-backlog.md`: open, edit, save files with Kate-identical
highlighting and themes; multi-cursor; undo; find/replace; tabs and splits;
sessions; `.kateproject` reading; modelines; folding; indentation for C-like
and Python; LSP diagnostics, completion, hover, go-to-definition; crash
recovery. No vi mode, no terminal, no git, no spell check, no printing.

A Kate user opening v1 should find their files highlighted exactly as before,
their project file understood, their LSP config working, and their fingers
mostly at home. Everything else is a packet.
