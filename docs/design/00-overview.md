# KateM5Rust design plan: overview

**Target:** macOS on Apple Silicon (`aarch64-apple-darwin`), macOS 14 or later.
**Upstream studied:** Kate `v26.08.2`, KTextEditor `v6.31.0`,
KSyntaxHighlighting `v6.31.0` (pinned under `upstream/`, read-only).
**Toolchain:** Rust `1.99.1`, pinned.
**Governance:** `docs/governance/GOVERNANCE_LINUS.md`. This plan obeys it:
types first, atomic packets, locked crate boundaries, every early packet a
Gate 1 payload that validates.

## What we are building

Kate is three things stacked: a text component (KTextEditor: buffer, undo,
cursors, rendering, highlighting, search, vi mode, completion, scripting), an
application shell (`kate/apps/lib`: windows, splits, tabs, sessions, sidebars,
quick open, output view), and a plugin zoo (`kate/addons`: 36 of them, from
LSP to an SQL client). That is roughly 170,000 lines of C++ on top of Qt and
KDE Frameworks.

We are not porting 170,000 lines. We are building a macOS text editor that a
Kate user recognizes: same buffer semantics, same highlighting (bit for bit,
because we load the same definitions), same sessions-and-splits workflow,
same `.kateproject` files, same LSP server configuration format. The parts
of Kate that exist because Qt or KDE needed them, or because someone wrote a
plugin once, are cut. The cut list is in `04-parity-map.md` and it is long on
purpose.

## How to read this plan

| File | Contents |
|---|---|
| `01-data-structures.md` | The Rust types, subsystem by subsystem, with the reasons. This is the plan. Everything else is commentary. |
| `02-crates.md` | The Cargo workspace: one crate per subsystem, dependency direction, what exists now. |
| `03-gui-toolkit.md` | The macOS GUI decision: GPUI, with the honest tradeoffs and the kill criteria. |
| `04-parity-map.md` | Kate subsystem to Rust module, with status: port, adapt, defer, cut. |
| `05-backlog.md` | Sequenced work packets, phases, lanes (local vs cloud), and why. |
| `06-licensing.md` | What (L)GPL upstream means for a rewrite that reads it, and the license recommendation. Owner decides. |
| `packets/PKT-*.json` | The first thirteen packets as Gate 1 payloads. They validate against `docs/governance/schemas/gate1-planning-handoff.schema.json`. |

## Decisions, in one place

These are the calls that cost real money to reverse. Each has a section in
the files above with the reasoning; this is the index.

| # | Decision | Where |
|---|---|---|
| D1 | Text storage is `ropey::Rope`, flat char offsets, line endings normalized to `\n` on load. | `01` §1 |
| D2 | One edit primitive: `Edit { start, old, new }`. Insert and delete are not operations. | `01` §1 |
| D3 | Positions that survive edits are `Anchor { offset, revision, bias }` resolved by replaying a bounded edit log. No observers, no `Rc<RefCell>`. | `01` §2 |
| D4 | Undo entries store forward and inverse transactions plus selection ranges; modified-state is undo-stack depth at save. | `01` §3 |
| D5 | One cursor is a selection set of size one. There is no single-cursor path. | `01` §4 |
| D6 | Syntax highlighting is a Rust implementation of the KSyntaxHighlighting XML format and engine, loading the upstream definitions as data. Not tree-sitter, not syntect. | `01` §5 |
| D7 | Regex for search is the linear-time `regex` crate over rope chunks; regex for highlighting is `fancy-regex` because the definitions need lookaround. Two engines, two reasons, not one abstraction. | `01` §5, §6 |
| D8 | Files: BOM, strict UTF-8, configured fallback; atomic save via temp file and rename; unmappable chars are errors. | `01` §7 |
| D9 | The editor model (`kate-core`) is one owning tree: `Workspace` owns documents and views in `SlotMap`s; the GUI calls `execute(view, Command)` and reads state. Events go out through an outbox `Vec<Event>`, not callbacks. | `01` §8 |
| D10 | Sessions are our own JSON; `.kateproject` files are read for compatibility; Kate's `.katesession` INI is not read. | `01` §9 |
| D11 | No dynamic plugins. "Plugins" are crates linked at build time behind the `Command`/`Event`/`Workspace` API. No ABI, no WASM, no scripting engine in v1. | `01` §10 |
| D12 | LSP client is synchronous Rust over threads and pipes, `lsp-types` for the wire types, configuration file-compatible with Kate's `lspclient` `settings.json`. | `01` §11 |
| D13 | GUI toolkit is GPUI, behind a `kate-ui` crate that owns no model state. Kill criteria and the AppKit fallback are written down before the first line. | `03` |
| D14 | Vi mode, completion, spell check, printing, scripting, and 30 of 36 addons are deferred or cut from v1. | `04` |
| D15 | Recommended license: GPL-3.0-or-later for the application, with core library crates under LGPL-2.1-or-later. Owner decides; no LICENSE file in this PR. | `06` |

## Principles the plan is written against

- **Data structures first.** Every section of `01` opens with types. If a
  type is not there, the feature is not planned.
- **The special case is a layout bug.** A buffer always has a last line. A
  selection set always has a primary. An insert is a replace. If a design
  needs `if n == 1`, the design is wrong.
- **Eliminate the observer.** Qt's signals and KTextEditor's `MovingCursor`
  feedback objects become plain values plus a log, or an outbox the shell
  drains. Rust's borrow checker is not an obstacle here; it is the reason
  this is simpler.
- **Measure before optimizing, but choose structures that can be measured
  into.** `Vec<LineMeta>` for per-line state is O(n) on line insertion. That
  is fine until it is not, and the API does not change when it becomes a
  chunked vector.
- **Do not break userspace.** The owner's `.kateproject` files, LSP server
  configs, Kate modelines in files, and muscle memory are userspace.
  Our own on-disk formats carry a `format_version` from day one.
