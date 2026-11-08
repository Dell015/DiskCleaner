//! The `dc` command-line tool.
//!
//! Status: Phase B0 prints its name and version only. It performs no file system access.
//! Subcommands arrive in later phases (`dc scan` in B1).

#![forbid(unsafe_code)]

fn main() {
    println!("dc {}", env!("CARGO_PKG_VERSION"));
}
