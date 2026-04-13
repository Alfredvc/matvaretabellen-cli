use crate::cli::FoodsCmd;
use crate::data;
use crate::output::{self, Output};
use crate::rda;
use crate::search::SearchIndex;
use crate::types::Locale;
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::sync::OnceLock;

pub fn run(cmd: &FoodsCmd, locale: Locale, out: &Output) -> Result<()> {
    match cmd {
        FoodsCmd::List => list(locale, out),
        FoodsCmd::Get { id } => get(locale, id, out),
        FoodsCmd::Search { query, limit } => search(locale, query, *limit, out),
        FoodsCmd::Rda { id, profile } => rda_coverage(locale, id, profile.as_deref(), out),
    }
}

fn list(locale: Locale, out: &Output) -> Result<()> {
    let cache = data::foods(locale);
    let value = serde_json::to_value(&cache.items)?;
    output::emit(out, value)
}

fn get(locale: Locale, id: &str, out: &Output) -> Result<()> {
    let cache = data::foods(locale);
    let idx = cache
        .by_id
        .get(id)
        .ok_or_else(|| anyhow!("food {id} not found in locale {}", locale.code()))?;
    let food = &cache.items[*idx];
    output::emit(out, serde_json::to_value(food)?)
}

fn search(locale: Locale, query: &str, limit: usize, out: &Output) -> Result<()> {
    let cache = data::foods(locale);
    let index = search_index_for(locale);
    let hits = index.search(query);
    let mut results: Vec<Value> = Vec::with_capacity(hits.len().min(limit));
    for idx in hits.into_iter().take(limit) {
        results.push(serde_json::to_value(&cache.items[idx])?);
    }
    output::emit(out, Value::Array(results))
}

fn rda_coverage(
    locale: Locale,
    food_id: &str,
    profile_id: Option<&str>,
    out: &Output,
) -> Result<()> {
    let foods_cache = data::foods(locale);
    let food_idx = foods_cache
        .by_id
        .get(food_id)
        .ok_or_else(|| anyhow!("food {food_id} not found in locale {}", locale.code()))?;
    let food = &foods_cache.items[*food_idx];

    let rda_cache = data::rda(locale);
    let profile = match profile_id {
        Some(id) => {
            let idx = rda_cache
                .by_id
                .get(id)
                .ok_or_else(|| anyhow!("rda profile {id} not found"))?;
            &rda_cache.items[*idx]
        }
        None => rda_cache
            .items
            .first()
            .ok_or_else(|| anyhow!("no rda profiles in dataset"))?,
    };

    let nutrients = &data::nutrients(locale).items;
    let coverage = rda::compute(food, profile, nutrients);
    output::emit(out, serde_json::to_value(coverage)?)
}

// Lazily-built per-locale search index, cached for repeat queries.
static SEARCH_NB: OnceLock<SearchIndex> = OnceLock::new();
static SEARCH_EN: OnceLock<SearchIndex> = OnceLock::new();

fn search_index_for(locale: Locale) -> &'static SearchIndex {
    let cell = match locale {
        Locale::Nb => &SEARCH_NB,
        Locale::En => &SEARCH_EN,
    };
    cell.get_or_init(|| SearchIndex::build(&data::foods(locale).items, locale.code()))
}
