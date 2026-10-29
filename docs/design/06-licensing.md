# Licensing: what reading Kate's code means for a Rust rewrite

**Status: the owner accepted the recommended split. PKT-0043 records it.
This file is not legal advice.**

## 0. Owner decision (2026-10-29)

The copyright holder is John Valdez, 2026.

| Part | License | Where the text lives |
|---|---|---|
| Application (`apps/katem5rust`) and workspace default | GPL-3.0-or-later | `LICENSE` and `LICENSES/GPL-3.0-or-later.txt` |
| Tool `gatecheck` | GPL-3.0-or-later | same GPL text |
| Core crates `kate-text`, `kate-fileio`, `kate-search`, `kate-core` | LGPL-2.1-or-later | `LICENSES/LGPL-2.1-or-later.txt` |
| `kate-syntax` | MIT | `LICENSES/MIT.txt` |

Every Rust file carries `SPDX-FileCopyrightText: 2026 John Valdez` and the
`SPDX-License-Identifier` for its crate. `cargo test -p gatecheck --test
spdx_headers` fails when a header is missing. CI runs that test.

The GPL grant has one additional term under GPL-3.0 section 7(b). If you
distribute a modified version of a GPL-3.0-or-later part, keep this line in
the About box or in the equivalent Appropriate Legal Notices:

    Based on KateM5Rust by John Valdez, a Rust rewrite of KDE Kate

Section 7(b) allows a term that preserves a reasonable author attribution.
The term does only that. It is not a further restriction under section 10.
It does not apply to the LGPL crates or to `kate-syntax` when those crates
are conveyed on their own. It does not add a term to the KDE works. The full
wording is at the top of `LICENSE` and in `NOTICE`.

The earlier note that `gatecheck` could be MIT or CC0 is withdrawn. The
workspace default is GPL-3.0-or-later, and `gatecheck` follows that default.

The analysis below is the record of why this split was recommended. It is
not legal advice.

## 1. What is upstream, exactly

Counted from `SPDX-License-Identifier` headers in the pinned submodules
(`rg -o "SPDX-License-Identifier: .*" | sort | uniq -c`).

### KTextEditor `v6.31.0` (`upstream/ktexteditor/src`)

| License | Files | Notable files |
|---|---|---|
| `LGPL-2.0-or-later` | 327 | buffer, undo, search, syntax glue, most of everything |
| `LGPL-2.0-only` | 16 | **`document/katedocument.cpp`, `view/kateview.cpp`, `view/kateviewinternal.cpp`, `render/katerenderer.{h,cpp}`, `view/kateviewhelpers.{h,cpp}`, `utils/kateautoindent.cpp`, `dialogs/katedialogs.cpp`**, and the JS indenters `cstyle.js`, `ada.js`, `pascal.js`, `ruby.js`, `xml.js`, libraries `string.js`, `utils.js` |
| `MIT` | 6 | small utilities |

The `-only` files are the heart of the document/view model and the C-style
indenter. Those are precisely the files an agent porting "the editor" reads
most.

### Kate `v26.08.2` (`upstream/kate/apps`, `upstream/kate/addons`)

| License | Files |
|---|---|
| `LGPL-2.0-or-later` | 396 (+8 with a malformed trailing header) |
| `LGPL-2.0-only` | 110 |
| `MIT` | 51 (including `kfts_fuzzy_match.h`, the quick-open matcher we plan to port) |
| `GPL-2.0-or-later` | 36 (`kate-ctags`, `katebuild-plugin`, `openlink`, `rbql`, `symbolviewer`, `template`, `textfilter`, `welcomeview`) |

### KSyntaxHighlighting `v6.31.0` (`upstream/syntax-highlighting`)

| Component | License |
|---|---|
| Library `src/lib` (49 files) | **MIT** |
| Themes `data/themes` (30 files) | **MIT** |
| Test harness `autotests/*.cpp` | MIT |
| Test inputs `autotests/input` (381 files) | mostly unmarked snippets; 11 carry SPDX: 7 MIT, 2 BSD-2-Clause, 1 LGPL-2.1-or-later, 1 GPL-2.0-only |
| Syntax definitions `data/syntax` (413 files, `license=` attribute) | **151 MIT**; 92 "LGPL" (no version); 25 LGPLv2+; 3 LGPLv2.1+; 3 LGPLv2; 4 "LGPL 2.1 or 3 … approved by KDE e.V."; 27 "GPL" (no version); 7 GPLv2+; 3 GPLv2; 2 GPLv3+; 16 **empty**; 4 WTFPL; 4 Public Domain; 2 BSD-3-Clause; 2 BSD; 1 New BSD; 2 Artistic; 1 zlib/libpng; 1 LLVM; 2 FDL |

Upstream itself treats the definitions as a licensing problem: its
`CMakeLists.txt` has `option(QRC_SYNTAX ...)` to "allow to install the
'differently' licensed syntax xml files instead of putting them in a QRC and
link them in". We inherit that problem unchanged.

## 2. Is a rewrite that *reads* this code a derivative work?

Copyright protects expression, not ideas, algorithms or interfaces. A
program written from a specification, or from observing behavior, is not a
derivative of the original. That is the "clean room" defense, and it depends
on the people writing the new code *not having read the old code*.

We do not have that defense and should not pretend to. The entire point of
`upstream/` is that agents read `katetextbuffer.cpp` and `kateundo.h` and
then write Rust. Where the Rust reproduces the *structure, sequence and
organization* of the C++ (same decomposition, same algorithm steps in the
same order, same corner-case handling, same comments paraphrased), a court
can find derivation even with zero lines copied. Where the Rust is a
different design that solves the same problem (a rope and a flat anchor log
instead of blocks and moving cursors; two transactions instead of seven undo
item types), derivation is much harder to argue, because the *expression* is
new and only the *ideas* (which are free) were taken.

`01-data-structures.md` is written to push as much of the project as
possible into the second category. It is still prudent to assume the
following:

- `kate-text`, `kate-core`, `kate-fileio`, `kate-search`: **possibly derived**
  from `LGPL-2.0-or-later` and `LGPL-2.0-only` KTextEditor sources.
- `kate-syntax` engine: **likely derived** from the MIT KSyntaxHighlighting
  engine, because the semantics of the XML format *are* that engine's
  behavior and the golden tests require identical output. MIT is permissive;
  the obligation is attribution. Keep the upstream copyright notice in
  `crates/kate-syntax/LICENSE-MIT-upstream` or equivalent.
- Vendored XML definitions and JSON themes: **copied**, not derived. Each
  carries its own license and we distribute under it.
- Indenter ports (`cstyle`): **derived** from `LGPL-2.0-only` JS.
- Quick-open fuzzy matcher: **derived** from MIT C++. Attribution.
- Anything from `kate/addons` we port: mostly LGPL-2.0-or-later; a few
  directories GPL-2.0-or-later (none of those are planned for v1).

## 3. What the LGPL-2.0 obligations are, concretely

LGPL-2.0 is written for *libraries* linked into applications. For a Rust
application that statically links its own derived crates, the relevant parts
are:

- **Section 2:** a modified version of the library (our derived crates) must
  itself be licensed under the LGPL, with source available. Fine; we publish
  source.
- **Section 3:** "You may opt to apply the terms of the ordinary GNU General
  Public License instead of this License to a given copy of the Library …
  If a newer version than version 2 of the ordinary GNU General Public
  License has appeared, then you can specify that version instead if you
  wish." This is the escape hatch: **any LGPL-2.0 code, `-only` included,
  may be relicensed by us to GPL-2.0 or any later GPL version.** The
  `-only` on `katedocument.cpp` therefore does not pin us to a GPL-2.0-only
  outcome; it pins us to *some* GPL or LGPL, our choice of version.
- **Section 6 (combined works):** an application linking the LGPL library
  must let the user relink against a modified library. For static linking
  that means shipping object files or source. If the whole application is
  itself GPL or LGPL, section 6 is moot: the user already has everything.

Conclusion: an LGPL-2.0-derived crate inside an open-source application is
not a problem. An LGPL-2.0-derived crate inside a *proprietary* macOS app is
a real constraint (relinking obligation with static linking, in practice:
ship as a dynamic library or ship object files). The owner should know that
a closed-source future is expensive with this codebase.

## 4. The dependency that forces the version question: GPUI

`gpui` is **Apache-2.0**. Apache-2.0 is compatible with GPL-3.0 and
incompatible with GPL-2.0-only (the FSF's position; the patent-termination
and indemnification clauses are the reason). Most of the Rust ecosystem is
`MIT OR Apache-2.0`, where we simply pick MIT; GPUI is Apache-2.0 *only*.

So, if the application is GPL at all, it must be **GPL-3.0 or
GPL-2.0-or-later** (where the distributor exercises the "or later" to
combine with Apache-2.0). `GPL-2.0-only` would make the GPUI decision
illegal. This is the single most important interaction in this document.

If the owner rejects GPUI for AppKit via `objc2` (MIT), this constraint
disappears, but the LGPL-2.0 derivation analysis in section 3 does not.

## 5. The syntax definitions

Three groups:

1. **MIT (151) and other permissive (BSD, zlib, LLVM, Public Domain, WTFPL):
   vendor freely**, keep headers, list in `SOURCES.md`. PKT-0007 vendors six
   of these; PKT-0028 the rest of this group.
2. **LGPL / GPL of any version (~160):** as *data files* loaded at runtime
   from the app bundle, they are an aggregation, not a derivative, and their
   own license applies to them alone. Embedding them in the binary with
   `include_str!` arguably makes the binary a combined work. Under a GPL
   application license this distinction stops mattering; under anything
   else, load them from `Contents/Resources/syntax/` as files, exactly the
   way upstream's `QRC_SYNTAX=OFF` does.
3. **Empty `license=""` (16) and FDL (2):** do not vendor until each is
   resolved upstream or dropped. Sixteen definitions nobody can name a
   license for are not worth a conversation with a lawyer.

The test corpus under `autotests/input` is unmarked snippets of real-world
code in 300 languages; only 11 files carry SPDX headers. Use the
`reference/` outputs for golden tests (they are generated by the MIT engine
from the inputs and are the project's own work) and vendor only inputs with
a permissive header or inputs we write ourselves. PKT-0008 is scoped to the
six MIT definitions for exactly this reason.

## 6. Options

| Option | Application license | Core crates | Works with GPUI | Works with LGPL-2.0-only derivation | Closed-source future | Notes |
|---|---|---|---|---|---|---|
| **A (recommended)** | **GPL-3.0-or-later** | **LGPL-2.1-or-later** (or LGPL-3.0-or-later) | yes | yes (via LGPL §3 upgrade) | no | Matches KDE's own practice (LGPL libs, GPL apps). Clean, boring, defensible. |
| B | GPL-2.0-or-later | LGPL-2.0-or-later | yes, but only because "or later" lets distributors apply GPL-3 terms when combining with Apache-2.0 | yes | no | Linus's preference in spirit; in practice every binary is distributed under GPL-3 terms anyway because of GPUI. Honest version of this is Option A. |
| C | MIT or Apache-2.0 | same | yes | **no** — requires a genuine clean room for everything derived from KTextEditor, which the agent workflow cannot provide | yes | Would require deleting `upstream/ktexteditor` and `upstream/kate`, forbidding agents from reading them, and re-deriving behavior from documentation and black-box testing only. Possible for `kate-syntax` (MIT upstream) alone. |
| D | Proprietary | LGPL core as separate dynamic libraries | yes (Apache-2.0 fine) | relinking obligation satisfied by dylibs | yes | Legally workable, operationally ugly (dylibs, symbol stability), and it loses the vendored GPL/LGPL definitions unless loaded as files. |
| E | Mixed: `kate-syntax` MIT, everything else GPL-3.0-or-later | — | yes | yes | no | Lets other Rust projects reuse the KSyntaxHighlighting port. Worth doing *within* Option A: publish `kate-syntax` under MIT since its upstream is MIT and it depends on nothing LGPL. |

## 7. Recommendation

**Option A with E folded in:**

- `katem5rust`, `kate-ui`, `kate-core`, `kate-text`, `kate-fileio`,
  `kate-search`, `kate-session`, `kate-lsp`, `kate-vimode`:
  **GPL-3.0-or-later** for the application binary; the library crates
  **LGPL-2.1-or-later** so that they remain combinable with the widest set
  of things (LGPL-2.1 §3 still permits upgrading to GPL; 2.1 is the version
  KDE Frameworks standardized on).
- `kate-syntax`: **MIT**, with upstream's copyright notice preserved,
  because its upstream is MIT and it has no LGPL ancestry. It becomes
  reusable by the wider Rust community, which is a real contribution.
- `gatecheck` follows the workspace default, **GPL-3.0-or-later**. Section 0
  records that choice. The earlier MIT-or-CC0 option for this tool is
  withdrawn. Governance documents and design documents stay with the
  repository license grant in `LICENSE`.
- Vendored definitions and themes: their own licenses, headers intact,
  `SOURCES.md` per directory, non-permissive definitions loaded as files
  from the bundle.
- Every source file gets an `SPDX-License-Identifier` header and the repo
  gets a `LICENSES/` directory, REUSE-style, once the owner decides. That is
  a gatekeeper packet.

Why not fight for permissive: the value of this project is *being Kate*,
including loading Kate's own definitions, reading Kate's project files, and
porting Kate's indenters. Every one of those pulls in copyleft. A permissive
license would require not looking at the thing we are rewriting, and the
honest name for that project is "a new editor", which is a fine project and
not this one.

## 8. What the owner must decide

1. **Decided.** The owner accepted Option A with the MIT exception for
   `kate-syntax`. PKT-0043 writes that decision into the tree. See section 0.
2. Whether the GPL/LGPL-licensed syntax definitions are vendored (as files)
   now, or only the permissive ones. This still blocks PKT-0028.
3. Whether a closed-source or App Store distribution is ever a goal. The
   accepted license does not leave that path open. A later change toward
   Option D would be a new decision.
4. Whether to make the repository public. GPL does not require it, but
   branch protection and free macOS CI minutes do
   (`GOVERNANCE_LINUS.md` §XI), and a GPL project with a private repo is an
   odd combination.

`upstream/` stays read-only. Agents still read it for behavior. They do not
transcribe it into Rust source.
