# GUI toolkit for macOS

## The decision

**GPUI** (Zed's UI framework, `gpui` on crates.io, Apache-2.0), confined to
one crate, `kate-ui`, that owns no editor state. Decided provisionally; the
spike packet PKT-0013 has written kill criteria. If it fails them, the
fallback is **AppKit through `objc2` + `objc2-app-kit` with a custom
`NSView` drawing via Core Text**, and `kate-core` does not change by one
line either way.

That last sentence is the actual architecture decision. The toolkit is a
leaf. Everything in `01-data-structures.md` is toolkit-blind, and the
packets are ordered so that eleven of the first twelve land before a single
window opens.

## What a text editor needs from a toolkit on macOS

Not "widgets". A code editor draws its own editor view in every toolkit that
has ever shipped one. What it needs is:

1. **Text rendering that is correct and fast on Retina**: Core Text shaping
   (ligatures, CJK, emoji, combining marks, fallback fonts), subpixel
   positioning, smooth scrolling of 100k-line files at 120 Hz on ProMotion.
2. **Input method support**: `NSTextInputClient` for dead keys, marked text,
   the Korean/Japanese/Chinese IMEs, and the press-and-hold accent popover.
   Without this the editor is unusable for a large share of users.
3. **Native windows, menus, the Dock, Cmd-key bindings, Services, open/save
   panels, drag and drop of files, Quick Look-ish behavior.** Users notice
   fake menus.
4. **A sane way to build the chrome**: tabs, splits, a find bar, a command
   palette, a file tree, settings. Hundreds of small widgets that should not
   each cost a day.
5. **Accessibility**: VoiceOver should at least read the current line.

## Candidates, honestly

| | GPUI | AppKit via `objc2` | iced | Floem | egui | Slint |
|---|---|---|---|---|---|---|
| Text shaping on macOS | Core Text, Metal rasterizer; proven in Zed | Core Text directly; you write the layout engine | `cosmic-text` (rustybuzz + swash); good, not Core Text; no subpixel | `cosmic-text` via Parley/Vello | own rasterizer; poor for editors | own; adequate |
| IME / `NSTextInputClient` | yes (Zed ships CJK) | yes, you implement the protocol | partial (winit) | partial (winit) | partial (winit) | partial |
| Native menus, panels | yes | yes | no native menus | no | no | no |
| Scroll performance at 100k lines | virtualized lists, Metal; known good | depends entirely on your code | fine with custom widget | fine | poor | unknown for this |
| Chrome productivity | high; Tailwind-like styling, flex layout, entities | low; AppKit from Rust is verbose and every view is a class | high (Elm) | medium | high | high (DSL) |
| API stability | **low**: 0.2.x, breaking changes between releases, docs thin, shaped by Zed's needs | high: AppKit does not change; `objc2` is stable | medium; semver, active | low | high | high |
| Dependency weight / build time | heavy (hundreds of crates; ~2–4 min clean build) | light | medium | medium | light | medium |
| Build on Linux (cloud agents) | **no**: `build.rs` needs `xcrun` for Metal shaders | no (needs SDK headers) | yes | yes | yes | yes |
| Cross-platform later | Linux/Windows in progress upstream | macOS only | yes | yes | yes | yes |
| Accessibility | limited, improving | full, if you implement it | limited | limited | limited | partial |
| License | Apache-2.0 | MIT (objc2) | MIT | MIT | MIT/Apache | GPL-3 / royalty-free / commercial |

## Why GPUI

The hardest problem in a macOS editor is item 1, and GPUI is the only Rust
toolkit where that problem is solved *for an editor* and shipping to a large
user base. Item 2 likewise. Items 3 and 4 are good. The cost is item 5
(stability) and the Linux build gap, both of which we contain:

- **Containment of instability.** `kate-ui` is the only crate that names a
  `gpui` type. Breaking GPUI releases are a `kate-ui` packet, period. The
  model, the tests, the golden files and the LSP client are unaffected.
- **Containment of the build gap.** `kate-ui` is a `LOCAL` lane crate. Cloud
  agents never build it. CI's `macos-15` job does. The Linux CI job tests
  everything below it. This matches the governance lane rules; it did not
  create them.
- **Pin and bump deliberately.** `gpui` is pinned to an exact version in
  `kate-ui/Cargo.toml`; bumping is a packet with the scroll benchmark re-run.

## Why not the others

- **AppKit via `objc2`** is the most "Mac" answer and the one we fall back
  to. We do not start there because it means writing a text layout engine
  (line breaking, bidi, glyph runs, caching) over Core Text before drawing
  the first line, plus every piece of chrome as an `NSView` subclass through
  Objective-C message sends. That is months of unsafe-adjacent FFI code
  written by agents, which is the worst possible combination. If GPUI fails
  the spike, we pay this price knowingly.
- **iced** is the stable, well-run alternative, and the right answer if the
  owner later wants Linux. It loses on native menus, IME maturity and text
  rendering parity with Core Text. For "macOS first, make it feel native",
  that is three strikes.
- **Floem** is Lapce's toolkit: built for an editor, but a smaller community
  than either GPUI or iced and the same IME and menu gaps as iced.
- **egui** is excellent for tools and wrong for a text editor: immediate-mode
  relayout of a large document every frame and non-native text.
- **Slint** has a licensing model (GPL-3 or commercial or "royalty-free" with
  conditions) that interacts badly with the undecided project license, and
  its text editing story is a basic widget.
- **A web view (Tauri)** would be an Electron-shaped editor in a Rust coat.
  No.

## Kill criteria for GPUI (evaluated at Gate 3 of PKT-0013)

The spike renders a read-only `Document` with themed spans and line numbers.
It is rejected, and the AppKit fallback spike is opened, if any of these
fail on the owner's machine:

1. p95 paint time above 8 ms while scrolling a 100,000-line generated Rust
   file (120 Hz budget with headroom), measured by the spike's own
   `KATEM5_PROFILE` JSON output.
2. IME marked text (Japanese input, dead-key accents) does not display inline.
3. Font fallback fails for CJK or emoji in the default monospace font.
4. The `gpui` crate at the pinned version does not build with the pinned
   toolchain on a clean checkout with `--locked`.
5. The owner looks at it and says no. That is a legitimate criterion for a
   GUI decision and the reason the lane is `LOCAL` with `OWNER_EYES`.

## What the GUI crate contains, and what it does not

Contains: `App` (owns the `Workspace`), `EditorElement` (paints one `View`),
`ShapedLineCache` keyed by `(line, Revision)`, the find bar, tabs and splits
drawn from `SplitTree`, the command palette, menus bound to `Command`s, the
file watcher thread, LSP client wiring, dialogs.

Does not contain: any decision about what text to insert, where a cursor
goes, what a save writes, or what color a keyword is. Those are answered by
`kate-core` and `kate-syntax`, and the GUI asks.
