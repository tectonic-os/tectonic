//! This module renders the written tree that `create`, `import` and `set`
//! share.
//!
//! A flow collects every answer before it writes, so no apply takes a prompt.
//! This renders the tree line that names what it wrote.

use crate::layout;
use common::ui::tree::Change;
use std::path::{Path, PathBuf};

/// A collect builds the text it says while it asks.
/// The caller prints that text.
pub type Say<'a> = &'a mut dyn FnMut(&str);

pub fn report(root: &Path, wrote: &[(PathBuf, Change)]) {
    let declared = crate::model::image::List::load(root).0.id;
    let id = match declared.is_empty() {
        true => layout::named_after_root(root).unwrap_or_default(),
        false => declared,
    };
    common::ui::tree::print(&id, wrote, describe);
}

/// A tree line says what a later step added to an existing file. File-purpose
/// descriptions stay in documentation so every path does not gain a column.
fn describe(_path: &Path, change: Option<&Change>) -> String {
    match change {
        Some(Change::Updated(edit)) => edit.clone(),
        _ => String::new(),
    }
}
