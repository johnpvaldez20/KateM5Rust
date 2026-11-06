# Lane split for the Cursor coding agents

This note says which machine codes which packets.
Three lanes code the rest of Phase 2 and all of Phase 3.
Every lane runs a Cursor agent.
No other bot codes a packet.

| Lane | Machine | Cursor mode | Builds | Owns |
|---|---|---|---|---|
| Framework | the Framework laptop | local execution | every crate except `kate-ui` and `apps` | `crates/kate-syntax/` |
| Mac | the owner Mac | local execution | every crate | `crates/kate-ui/`, `apps/`, and the pure Rust packets that the backlog marks LOCAL |
| Cloud | a Cursor cloud VM | cloud agent | every crate except `kate-ui` and `apps` | crate creation, `kate-lsp`, the recovery journal, and every packet that edits `workspace.rs` |

The `lane.runtime` field of each packet names the lane.
`LOCAL` with a `kate-syntax` path is the Framework lane.
`LOCAL` with any other path is the Mac lane.
`CLOUD` is the cloud lane.
`EITHER` goes to the first idle lane that can build the crate.

## Rules that keep the lanes apart

1. Two packets that are in flight at the same time do not share a file.
   `gatecheck` rejects two packets with overlapping `allowed_files` unless one depends on the other.
   A packet that depends on another waits until that packet is on main.
2. Each lane works in its own clone or worktree.
   The Framework lane uses `~/code/KateM5Rust-mit-<wave>` for one copy wave and `~/code/KateM5Rust-wire` for the wiring chain.
   The Mac lane uses `~/code/KateM5Rust`.
   A cloud agent gets a fresh VM.
3. One packet is one branch `pkt/PKT-NNNN-<slug>` and one pull request.
   Rebase on main before the pull request opens.
4. A lane does not start a packet whose `depends_on_packets` are not all on main.
5. When one lane is idle and another is full, the idle lane may take a packet from the other lane when it can build the crate.
   The Framework lane does not take a `kate-ui` or `apps` packet.
   A cloud agent does not take a `kate-ui` or `apps` packet.

## Framework lane

The Framework lane copies the MIT syntax files and wires them.

| Work | Packets | Index | Parallel |
|---|---|---|---|
| MIT copy wave 4 | PKT-0275 to PKT-0282 | `WAVE-0275.md` | eight at once |
| MIT copy waves 5 to 15 | PKT-0283 to PKT-0366 | `WAVE-0283.md` to `WAVE-0363.md` | eight at once per wave |
| vendored list and four engine fixes | PKT-0367 to PKT-0371 | `WAVE-0367.md` | PKT-0367 first, then PKT-0368 to PKT-0371 together |
| include_str wiring, 116 packets | PKT-0373 to PKT-0488 | `WAVE-0367.md` to `WAVE-0485.md` | one at a time |

The copy packets of one wave do not share a file.
Eight Cursor local agents can run at once, one per file, in the wave worktree.
The next copy wave can start before the wiring of the earlier wave is done.
The wiring packets share `vendored.rs` and `SOURCES.md`, so they run one after the other.
One wiring packet is one include line, one table row, and one test.

Recommended first batch: PKT-0275 to PKT-0282, then PKT-0367, then PKT-0368 to PKT-0371, then PKT-0373 onward.

## Mac lane

The Mac lane finishes the window and codes the Phase 3 panes.

| Work | Index | Lane field |
|---|---|---|
| Phase 2 window remnants: tabs, splits, find bar, mouse carets, session file, file watcher, theme, gutter, folds, wrap, config | `WAVE-0489.md` | LOCAL |
| the window loads every vendored definition | PKT-0372 in `WAVE-0367.md` | LOCAL |
| project tree, document list, quick open, command palette | `WAVE-0031.md` | LOCAL |
| diagnostics pane, hover, completion popup, go to definition | `WAVE-0035.md` | LOCAL |
| search in files pane, the indenter packets marked EITHER | `WAVE-0039.md` | LOCAL and EITHER |
| settings window, `.app` bundle | `WAVE-0041.md` | LOCAL |

A `kate-ui` packet that paints needs the owner to look at the window.
Its `lane.requires` lists `GUI_DISPLAY` and `OWNER_EYES`.
A `kate-ui` packet that only adds a model file lists `MACOS_HOST`, because the crate tests run in the `rust (macos-15)` job.

Recommended first batch: the LOCAL packets of `WAVE-0489.md` that have no open dependency, then PKT-0372.

## Cloud lane

The cloud lane codes the pure Rust packets that need a large context or that create a crate.

| Work | Index |
|---|---|
| `kate-core` commands for the window: CheckDisk, SelectNextOccurrence, MoveTo, ReplaceRanges, ToggleFold, layout methods | `WAVE-0489.md` |
| `kate-session` crate and the `.kateproject` reader | `WAVE-0030.md` |
| `kate-lsp` crate, JSON-RPC, client, diagnostics, requests | `WAVE-0033.md`, `WAVE-0034.md`, `WAVE-0035.md` |
| recovery journal and stash | `WAVE-0038.md` |

Recommended first batch: the CLOUD packets of `WAVE-0489.md` in id order, then PKT-0512 and PKT-0030, then PKT-0033.

## Order of the waves

Phase 2 ends when every packet of `WAVE-0489.md`, the fifteen copy waves, and the fifteen wiring waves are on main.
Phase 3 waves can start before Phase 2 ends when their `depends_on_packets` are on main.
`WAVE-0030.md` and `WAVE-0033.md` have no dependency on the window remnants.
`WAVE-0031.md`, `WAVE-0035.md`, `WAVE-0039.md`, and `WAVE-0041.md` paint panes, so they wait for the tab and split packets of `WAVE-0489.md`.

## Provenance for the Framework machine

`docs/governance/schemas/common.schema.json` knows two hosts: `OWNER_MACBOOK` and `CURSOR_CLOUD`.
A `LOCAL` worker must record `OWNER_MACBOOK`.
The Framework machine is not a MacBook.
PKT-0555 in `WAVE-0041.md` adds the host kind `OWNER_LINUX` for it.
Until that packet is on main, a Gate 2 payload for a Framework packet is signed by the gatekeeper with the Framework host named in `host.name`.

## MIT files outside the queue

The 116-file queue in `WAVE-0024.md` does not hold every upstream file with `license="MIT"`.
These 29 files also have that attribute at the pin:

`apparmor.xml`, `asciidoc.xml`, `csv.xml`, `doxyfile.xml`, `dts.xml`, `elvish.xml`, `fortran-fixed.xml`, `jam.xml`, `javascript-react.xml`, `julia.xml`, `logcat.xml`, `logfile-advanced.xml`, `lrc.xml`, `mermaid.xml`, `mustache.xml`, `orgmode.xml`, `racket.xml`, `raku.xml`, `sas.xml`, `scheme.xml`, `selinux-cil.xml`, `selinux-fc.xml`, `selinux.xml`, `snakemake.xml`, `subrip-subtitles.xml`, `systemd-unit.xml`, `typescript-react.xml`, `typescript.xml`, `zsh.xml`.

No packet copies them.
The owner decides whether a wave 16 and later copy them.
A dry run of the engine shows that `julia.xml`, `logcat.xml`, and `mermaid.xml` need more engine work before they can be wired.
