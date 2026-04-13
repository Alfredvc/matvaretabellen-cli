//! Embedded-data cache layer.
//!
//! Every resource is compressed at build time by `build.rs` into `$OUT_DIR/...json.zst`,
//! embedded via `include_bytes!`, decompressed on first access with `zstd::decode_all`,
//! parsed, indexed, and cached behind a `OnceLock`. Decompression / parse failures here
//! indicate either a corrupted build artifact or upstream schema drift — both are bugs,
//! so we panic with `.expect(...)` rather than surface errors to the caller.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::types::{
    Food, FoodGroup, FoodGroupsEnvelope, FoodsEnvelope, LangualCode, LangualEnvelope, Locale,
    Nutrient, NutrientsEnvelope, RdaEnvelope, RdaProfile, Source, SourcesEnvelope,
};

/// Data snapshot version (ISO date from `data/VERSION`) — emitted by `build.rs`.
pub const DATA_VERSION: &str = env!("MVT_DATA_VERSION");

// --- Cache structs -----------------------------------------------------------

pub struct FoodsCache {
    pub items: Vec<Food>,
    pub by_id: HashMap<String, usize>,
}

pub struct FoodGroupsCache {
    pub items: Vec<FoodGroup>,
    pub by_id: HashMap<String, usize>,
}

pub struct NutrientsCache {
    pub items: Vec<Nutrient>,
    pub by_id: HashMap<String, usize>,
}

pub struct SourcesCache {
    pub items: Vec<Source>,
    pub by_id: HashMap<String, usize>,
}

pub struct RdaCache {
    pub items: Vec<RdaProfile>,
    pub by_id: HashMap<String, usize>,
}

pub struct LangualCache {
    pub items: Vec<LangualCode>,
    pub by_id: HashMap<String, usize>,
}

// --- Embedded bytes ----------------------------------------------------------

const FOODS_NB_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/nb/foods.json.zst"));
const FOODS_EN_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/en/foods.json.zst"));

const FOOD_GROUPS_NB_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/nb/food-groups.json.zst"));
const FOOD_GROUPS_EN_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/en/food-groups.json.zst"));

const NUTRIENTS_NB_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/nb/nutrients.json.zst"));
const NUTRIENTS_EN_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/en/nutrients.json.zst"));

const SOURCES_NB_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/nb/sources.json.zst"));
const SOURCES_EN_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/en/sources.json.zst"));

const RDA_NB_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/nb/rda.json.zst"));
const RDA_EN_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/en/rda.json.zst"));

const LANGUAL_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/langual.json.zst"));

// --- OnceLocks ---------------------------------------------------------------

static FOODS_NB: OnceLock<FoodsCache> = OnceLock::new();
static FOODS_EN: OnceLock<FoodsCache> = OnceLock::new();

static FOOD_GROUPS_NB: OnceLock<FoodGroupsCache> = OnceLock::new();
static FOOD_GROUPS_EN: OnceLock<FoodGroupsCache> = OnceLock::new();

static NUTRIENTS_NB: OnceLock<NutrientsCache> = OnceLock::new();
static NUTRIENTS_EN: OnceLock<NutrientsCache> = OnceLock::new();

static SOURCES_NB: OnceLock<SourcesCache> = OnceLock::new();
static SOURCES_EN: OnceLock<SourcesCache> = OnceLock::new();

static RDA_NB: OnceLock<RdaCache> = OnceLock::new();
static RDA_EN: OnceLock<RdaCache> = OnceLock::new();

static LANGUAL: OnceLock<LangualCache> = OnceLock::new();

// --- Helpers -----------------------------------------------------------------

fn decompress(bytes: &[u8], resource: &str) -> Vec<u8> {
    zstd::decode_all(bytes)
        .unwrap_or_else(|e| panic!("embedded {resource} data corrupt (zstd decode failed): {e}"))
}

// --- Accessors ---------------------------------------------------------------

pub fn foods(locale: Locale) -> &'static FoodsCache {
    let cell = match locale {
        Locale::Nb => &FOODS_NB,
        Locale::En => &FOODS_EN,
    };
    cell.get_or_init(|| {
        let bytes = match locale {
            Locale::Nb => FOODS_NB_BYTES,
            Locale::En => FOODS_EN_BYTES,
        };
        let decoded = decompress(bytes, "foods");
        let envelope: FoodsEnvelope =
            serde_json::from_slice(&decoded).expect("embedded foods schema drift");
        let items = envelope.foods;
        let by_id = items
            .iter()
            .enumerate()
            .map(|(i, f)| (f.food_id.clone(), i))
            .collect();
        FoodsCache { items, by_id }
    })
}

pub fn food_groups(locale: Locale) -> &'static FoodGroupsCache {
    let cell = match locale {
        Locale::Nb => &FOOD_GROUPS_NB,
        Locale::En => &FOOD_GROUPS_EN,
    };
    cell.get_or_init(|| {
        let bytes = match locale {
            Locale::Nb => FOOD_GROUPS_NB_BYTES,
            Locale::En => FOOD_GROUPS_EN_BYTES,
        };
        let decoded = decompress(bytes, "food-groups");
        let envelope: FoodGroupsEnvelope =
            serde_json::from_slice(&decoded).expect("embedded food-groups schema drift");
        let items = envelope.food_groups;
        let by_id = items
            .iter()
            .enumerate()
            .map(|(i, g)| (g.food_group_id.clone(), i))
            .collect();
        FoodGroupsCache { items, by_id }
    })
}

pub fn nutrients(locale: Locale) -> &'static NutrientsCache {
    let cell = match locale {
        Locale::Nb => &NUTRIENTS_NB,
        Locale::En => &NUTRIENTS_EN,
    };
    cell.get_or_init(|| {
        let bytes = match locale {
            Locale::Nb => NUTRIENTS_NB_BYTES,
            Locale::En => NUTRIENTS_EN_BYTES,
        };
        let decoded = decompress(bytes, "nutrients");
        let envelope: NutrientsEnvelope =
            serde_json::from_slice(&decoded).expect("embedded nutrients schema drift");
        let items = envelope.nutrients;
        let by_id = items
            .iter()
            .enumerate()
            .map(|(i, n)| (n.nutrient_id.clone(), i))
            .collect();
        NutrientsCache { items, by_id }
    })
}

pub fn sources(locale: Locale) -> &'static SourcesCache {
    let cell = match locale {
        Locale::Nb => &SOURCES_NB,
        Locale::En => &SOURCES_EN,
    };
    cell.get_or_init(|| {
        let bytes = match locale {
            Locale::Nb => SOURCES_NB_BYTES,
            Locale::En => SOURCES_EN_BYTES,
        };
        let decoded = decompress(bytes, "sources");
        let envelope: SourcesEnvelope =
            serde_json::from_slice(&decoded).expect("embedded sources schema drift");
        let items = envelope.sources;
        let by_id = items
            .iter()
            .enumerate()
            .map(|(i, s)| (s.source_id.clone(), i))
            .collect();
        SourcesCache { items, by_id }
    })
}

pub fn rda(locale: Locale) -> &'static RdaCache {
    let cell = match locale {
        Locale::Nb => &RDA_NB,
        Locale::En => &RDA_EN,
    };
    cell.get_or_init(|| {
        let bytes = match locale {
            Locale::Nb => RDA_NB_BYTES,
            Locale::En => RDA_EN_BYTES,
        };
        let decoded = decompress(bytes, "rda");
        let envelope: RdaEnvelope =
            serde_json::from_slice(&decoded).expect("embedded rda schema drift");
        let items = envelope.profiles;
        let by_id = items
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.clone(), i))
            .collect();
        RdaCache { items, by_id }
    })
}

pub fn langual() -> &'static LangualCache {
    LANGUAL.get_or_init(|| {
        let decoded = decompress(LANGUAL_BYTES, "langual");
        let envelope: LangualEnvelope =
            serde_json::from_slice(&decoded).expect("embedded langual schema drift");
        let items = envelope.codes;
        let by_id = items
            .iter()
            .enumerate()
            .map(|(i, c)| (c.langual_code.clone(), i))
            .collect();
        LangualCache { items, by_id }
    })
}

// --- Tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foods_loads_both_locales() {
        for locale in [Locale::Nb, Locale::En] {
            let cache = foods(locale);
            assert!(!cache.items.is_empty(), "no foods for {}", locale.code());
            assert_eq!(cache.items.len(), cache.by_id.len());
        }
    }

    #[test]
    fn food_groups_loads_both_locales() {
        for locale in [Locale::Nb, Locale::En] {
            let cache = food_groups(locale);
            assert!(!cache.items.is_empty());
            assert_eq!(cache.items.len(), cache.by_id.len());
        }
    }

    #[test]
    fn nutrients_loads_both_locales() {
        for locale in [Locale::Nb, Locale::En] {
            let cache = nutrients(locale);
            assert!(!cache.items.is_empty());
            assert_eq!(cache.items.len(), cache.by_id.len());
        }
    }

    #[test]
    fn sources_loads_both_locales() {
        for locale in [Locale::Nb, Locale::En] {
            let cache = sources(locale);
            assert!(!cache.items.is_empty());
            assert_eq!(cache.items.len(), cache.by_id.len());
        }
    }

    #[test]
    fn rda_loads_both_locales() {
        for locale in [Locale::Nb, Locale::En] {
            let cache = rda(locale);
            assert!(!cache.items.is_empty());
            assert_eq!(cache.items.len(), cache.by_id.len());
        }
    }

    #[test]
    fn langual_loads() {
        let cache = langual();
        assert!(!cache.items.is_empty());
        assert_eq!(cache.items.len(), cache.by_id.len());
    }

    #[test]
    fn data_version_looks_like_iso_date() {
        // Canary: if build.rs ever emits an empty or malformed version, this
        // catches it before a release. Format: YYYY-MM-DD.
        let v = DATA_VERSION;
        assert_eq!(v.len(), 10, "unexpected version format: {v:?}");
        assert_eq!(&v[4..5], "-");
        assert_eq!(&v[7..8], "-");
    }
}
