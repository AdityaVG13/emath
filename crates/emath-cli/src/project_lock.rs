//! Production check/build/run refuse a malformed meaning lock without
//! depending on genesis/store/world crates.

use std::path::Path;

use crate::portfolio::MeaningLock;
use crate::{CliExit, EXIT_REFUSED};

/// Refuse when an existing lock file next to `source` does not parse.
pub fn refuse_malformed_project_lock(source: &Path) -> Option<CliExit> {
    let root = MeaningLock::discover_project_root(source);
    match MeaningLock::load(&root) {
        Ok(_) => None,
        Err(error) => {
            eprintln!("{error}");
            Some(EXIT_REFUSED)
        }
    }
}
