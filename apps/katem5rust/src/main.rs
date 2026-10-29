//! KateM5Rust application entry point.
//!
//! There is no editor here yet. The GUI shell arrives with its own packets
//! (see `docs/design/05-backlog.md`). Until then this binary only proves that
//! the workspace builds on the pinned toolchain.

fn main() {
    println!("katem5rust {}", env!("CARGO_PKG_VERSION"));
}
