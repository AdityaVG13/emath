use super::{CliExit, Path, constructor_inspect, run::not_a_checkpoint};

pub(crate) fn inspect(path: &Path, json: bool) -> CliExit {
    constructor_inspect(path, json).unwrap_or_else(|| not_a_checkpoint(path, json))
}
