use crate::cli::SourcesCmd;
use crate::data;
use crate::output::{self, Output};
use crate::types::Locale;
use anyhow::{anyhow, Result};

pub fn run(cmd: &SourcesCmd, locale: Locale, out: &Output) -> Result<()> {
    match cmd {
        SourcesCmd::List => output::emit(out, serde_json::to_value(&data::sources(locale).items)?),
        SourcesCmd::Get { id } => {
            let cache = data::sources(locale);
            let idx = cache
                .by_id
                .get(id)
                .ok_or_else(|| anyhow!("source {id} not found in locale {}", locale.code()))?;
            output::emit(out, serde_json::to_value(&cache.items[*idx])?)
        }
    }
}
