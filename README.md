# KateM5Rust

A rewrite of the [KDE Kate](https://kate-editor.org/) text editor in Rust,
for macOS on Apple Silicon first. Built by a hybrid swarm of local and cloud
agents under a strict gatekeeper, with GitHub as the only source of truth.

**Status:** governance, design plan and workspace skeleton. No editor code yet.

| Start here | |
|---|---|
| `docs/governance/GOVERNANCE_LINUS.md` | The rules. Laws, gates, lanes, handoffs. Read before touching anything. |
| `docs/design/00-overview.md` | The plan. Decisions index, then data structures, crates, GUI toolkit, parity map, backlog, licensing. |
| `docs/design/packets/` | The first thirteen work packets as Gate 1 payloads. |
| `upstream/` | Kate, KTextEditor and KSyntaxHighlighting as read-only pinned submodules. Never built. |

## Build

Rust `1.99.1` is pinned by `rust-toolchain.toml`; `rustup` installs it on
first use.

```sh
cargo build --workspace --locked
cargo test --workspace --locked      # includes gatecheck over the governance payloads
cargo run -p gatecheck --locked      # validate schemas, examples and packets by hand
```

The upstream submodules are not needed to build. To read them:

```sh
git submodule update --init --depth 1 upstream/kate upstream/ktexteditor upstream/syntax-highlighting
```

## License

The copyright holder is John Valdez, 2026.

| Part | License |
|---|---|
| Application and workspace default | GPL-3.0-or-later (`LICENSE`) |
| Core crates `kate-text`, `kate-fileio`, `kate-search`, `kate-core` | LGPL-2.1-or-later (`LICENSES/LGPL-2.1-or-later.txt`) |
| `kate-syntax` | MIT (`LICENSES/MIT.txt`) |

If you distribute a modified version of a GPL-3.0-or-later part, keep the
attribution below. Put it in the About box or in the equivalent Appropriate
Legal Notices:

    Based on KateM5Rust by John Valdez, a Rust rewrite of KDE Kate

That duty is an additional term under GPL-3.0 section 7(b). It preserves a
reasonable author attribution. It is not a further restriction. The full
term is in `LICENSE` and `NOTICE`.

## Credits

John Valdez is the author of the Rust work.

KateM5Rust is a Rust rewrite derived from study of KDE Kate, KTextEditor,
and KSyntaxHighlighting. The KDE contributors wrote those works. Their names
are in the upstream SPDX headers and in the upstream commit history. See
`AUTHORS` and `NOTICE`.
