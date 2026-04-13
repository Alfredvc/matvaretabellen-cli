use crate::cli::FoodGroupsCmd;
use crate::data;
use crate::output::{self, Output};
use crate::types::Locale;
use anyhow::{anyhow, Result};

pub fn run(cmd: &FoodGroupsCmd, locale: Locale, out: &Output) -> Result<()> {
    match cmd {
        FoodGroupsCmd::List => {
            output::emit(out, serde_json::to_value(&data::food_groups(locale).items)?)
        }
        FoodGroupsCmd::Get { id } => {
            let cache = data::food_groups(locale);
            let idx = cache
                .by_id
                .get(id)
                .ok_or_else(|| anyhow!("food-group {id} not found in locale {}", locale.code()))?;
            output::emit(out, serde_json::to_value(&cache.items[*idx])?)
        }
    }
}
