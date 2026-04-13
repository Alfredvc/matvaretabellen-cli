use crate::cli::RdaCmd;
use crate::data;
use crate::output::{self, Output};
use crate::types::Locale;
use anyhow::{anyhow, Result};

pub fn run(cmd: &RdaCmd, locale: Locale, out: &Output) -> Result<()> {
    match cmd {
        RdaCmd::List => output::emit(out, serde_json::to_value(&data::rda(locale).items)?),
        RdaCmd::Get { id } => {
            let cache = data::rda(locale);
            let idx = cache
                .by_id
                .get(id)
                .ok_or_else(|| anyhow!("rda profile {id} not found in locale {}", locale.code()))?;
            output::emit(out, serde_json::to_value(&cache.items[*idx])?)
        }
    }
}
