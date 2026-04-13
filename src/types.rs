use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Locale {
    Nb,
    En,
}

impl Locale {
    pub fn code(self) -> &'static str {
        match self {
            Locale::Nb => "nb",
            Locale::En => "en",
        }
    }
}

impl std::str::FromStr for Locale {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "nb" | "no" => Ok(Locale::Nb),
            "en" => Ok(Locale::En),
            other => anyhow::bail!("unknown locale {other:?}; expected nb or en"),
        }
    }
}

/// Full food record — deserialized directly from `/api/{locale}/foods.json`.
/// The documented bulk endpoint already carries every field we need (including
/// `searchKeywords`), so we skip the undocumented `compact-foods.json`.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Food {
    pub food_id: String,
    pub food_group_id: String,
    pub food_name: String,
    #[serde(default)]
    pub latin_name: Option<String>,
    #[serde(default)]
    pub uri: Option<String>,
    #[serde(default)]
    pub search_keywords: Vec<String>,
    #[serde(default)]
    pub energy: Option<SourcedQuantity>,
    #[serde(default)]
    pub calories: Option<SourcedQuantity>,
    #[serde(default)]
    pub edible_part: Option<EdiblePart>,
    #[serde(default)]
    pub portions: Vec<Portion>,
    #[serde(default)]
    pub langual_codes: Vec<String>,
    #[serde(default)]
    pub constituents: Vec<FoodConstituent>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SourcedQuantity {
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub quantity: Option<f64>,
    #[serde(default)]
    pub unit: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct EdiblePart {
    #[serde(default)]
    pub percent: Option<f64>,
    #[serde(default)]
    pub source_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Portion {
    pub portion_name: String,
    pub portion_unit: String,
    pub quantity: f64,
    pub unit: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FoodConstituent {
    pub nutrient_id: String,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub quantity: Option<f64>,
    #[serde(default)]
    pub unit: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FoodGroup {
    pub food_group_id: String,
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Nutrient {
    pub nutrient_id: String,
    pub name: String,
    pub unit: String,
    #[serde(default)]
    pub decimal_precision: Option<u32>,
    #[serde(default)]
    pub euro_fir_id: Option<String>,
    #[serde(default)]
    pub euro_fir_name: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub uri: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub source_id: String,
    pub description: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LangualCode {
    pub langual_code: String,
    pub description: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RdaProfile {
    pub id: String,
    pub demographic: String,
    pub energy_recommendation: (f64, String),
    #[serde(default)]
    pub kcal_recommendation: Option<f64>,
    pub recommendations: HashMap<String, Recommendation>,
}

/// Upstream encodes recommendation as any combination of keys. Keep all optional so
/// we don't fail deserialization on edge profiles.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    #[serde(default)]
    pub average_amount: Option<(f64, String)>,
    #[serde(default)]
    pub min_amount: Option<(f64, String)>,
    #[serde(default)]
    pub max_amount: Option<(f64, String)>,
    #[serde(default)]
    pub min_energy_pct: Option<f64>,
    #[serde(default)]
    pub max_energy_pct: Option<f64>,
    #[serde(default)]
    pub average_energy_pct: Option<f64>,
}

/// Wire envelopes — bulk endpoints wrap their payload in a single top-level key.
#[derive(Deserialize)]
pub struct FoodGroupsEnvelope {
    #[serde(rename = "foodGroups")]
    pub food_groups: Vec<FoodGroup>,
}

#[derive(Deserialize)]
pub struct NutrientsEnvelope {
    pub nutrients: Vec<Nutrient>,
}

#[derive(Deserialize)]
pub struct SourcesEnvelope {
    pub sources: Vec<Source>,
}

#[derive(Deserialize)]
pub struct LangualEnvelope {
    pub codes: Vec<LangualCode>,
}

#[derive(Deserialize)]
pub struct RdaEnvelope {
    pub profiles: Vec<RdaProfile>,
}

#[derive(Deserialize)]
pub struct FoodsEnvelope {
    pub foods: Vec<Food>,
    #[allow(dead_code)]
    pub locale: Option<String>,
}
