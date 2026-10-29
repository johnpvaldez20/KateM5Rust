# upstream/ — read-only reference sources

These are git submodules pinned to stable upstream tags. They exist so that
agents can **read** the original C++ while writing the Rust rewrite. They are
reference material, nothing else.

| Submodule | Upstream | Pinned tag | Commit |
|---|---|---|---|
| `upstream/kate` | https://invent.kde.org/utilities/kate | `v26.08.2` | `2b6465a886b9d3bfc6eab25065c14ffe025e6ac3` |
| `upstream/ktexteditor` | https://invent.kde.org/frameworks/ktexteditor | `v6.31.0` | `ea14509c132f2c016d55b38f7a1ec24cc6f9e7d2` |
| `upstream/syntax-highlighting` | https://invent.kde.org/frameworks/syntax-highlighting | `v6.31.0` | `6d8ab7945958e7354295db7f399cce06f7de7571` |

## Rules

1. **Nothing in here is ever built.** No `Cargo.toml` references `upstream/`.
   The Cargo workspace lists `upstream` in `exclude`. CI checks out the
   repository with `submodules: false` and asserts the directories are empty.
2. **Nothing in here is ever modified.** `.gitmodules` sets `update = none`
   and `ignore = all`. A diff that touches `upstream/` is a `SCOPE_VIOLATION`
   at Gate 2 (see `docs/governance/GOVERNANCE_LINUS.md`).
3. **Bumping a pin is its own packet.** One commit, one submodule, one tag,
   and the licensing table in `docs/design/06-licensing.md` re-checked.
4. **Read the license before you read the code.** Kate and KTextEditor are
   (L)GPL. KSyntaxHighlighting is MIT. What you learn here shapes what the
   Rust code can be licensed as. See `docs/design/06-licensing.md`.

## Getting the sources locally

```sh
git submodule update --init --depth 1 upstream/kate upstream/ktexteditor upstream/syntax-highlighting
```

`update = none` in `.gitmodules` means a plain `git submodule update --init`
skips them on purpose. You must name them explicitly, as above. Cloud agents
usually do not need them at all; the design documents already cite the
relevant files by path.
