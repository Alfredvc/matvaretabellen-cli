use crate::cli::LangualCmd;
use crate::data;
use crate::output::{self, Output};
use anyhow::{anyhow, Result};

pub fn run(cmd: &LangualCmd, out: &Output) -> Result<()> {
    match cmd {
        LangualCmd::List => output::emit(out, serde_json::to_value(&data::langual().items)?),
        LangualCmd::Get { id } => {
            let cache = data::langual();
            let idx = cache
                .by_id
                .get(id)
                .ok_or_else(|| anyhow!("langual code {id} not found"))?;
            output::emit(out, serde_json::to_value(&cache.items[*idx])?)
        }
    }
}
