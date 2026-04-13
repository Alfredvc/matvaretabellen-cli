use crate::cli::NutrientsCmd;
use crate::data;
use crate::output::{self, Output};
use crate::types::Locale;
use anyhow::{anyhow, Result};

pub fn run(cmd: &NutrientsCmd, locale: Locale, out: &Output) -> Result<()> {
    match cmd {
        NutrientsCmd::List => {
            output::emit(out, serde_json::to_value(&data::nutrients(locale).items)?)
        }
        NutrientsCmd::Get { id } => {
            let cache = data::nutrients(locale);
            let idx = cache
                .by_id
                .get(id)
                .ok_or_else(|| anyhow!("nutrient {id} not found in locale {}", locale.code()))?;
            output::emit(out, serde_json::to_value(&cache.items[*idx])?)
        }
    }
}
