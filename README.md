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

Undecided. See `docs/design/06-licensing.md` for the analysis and the
recommendation; the owner decides. Until then this repository carries no
license grant.
