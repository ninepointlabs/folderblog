//! Built-in stress content for testing themes (`--fixtures NAME`).

use include_dir::{Dir, include_dir};

pub static FIXTURES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/fixtures");
pub static DEFAULT_THEME: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/themes/default");

pub fn names() -> Vec<String> {
    let mut v: Vec<String> = FIXTURES.dirs().map(|d| d.path().to_string_lossy().to_string()).collect();
    v.sort();
    v
}
