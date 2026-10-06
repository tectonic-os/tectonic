//! Every command, over this repository and over the fixture repositories,
//! compared byte for byte against a committed snapshot.
//!
//! Review changed snapshots with `cargo insta review`.
//!
//! No test changes the process working directory: a fixture root is passed to
//! the tool as a path from the package root, so tests run in any order and the
//! path a diagnostic prints is the same on any machine.

mod docs;
mod flows;
mod harness;
mod repo;
mod reports;
