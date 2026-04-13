//! RDA (recommended daily allowance) coverage calculator.
//!
//! Given a food and a profile, compute for every nutrient the profile
//! recommends whether the food covers it — in one of three shapes:
//!
//! * `Average` — the profile recommends an average amount. Emit
//!   `percent = 100 * food / rec`.
//! * `MinMax`  — the profile recommends a range. Emit bounds + `in_range`.
//! * `Energy`  — the profile recommends a share of total energy (min, max, or
//!   average). Emit `energy_pct` and whether it falls inside the bounds.
//!   See the comment on [`energy_factor`] for the (approximate) mapping.
//!
//! Pure; no I/O. The caller passes in parsed data slices.

use crate::types::{Food, FoodConstituent, Nutrient, RdaProfile, Recommendation};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CoverageKind {
    Average,
    MinMax,
    Energy,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoverageEntry {
    #[serde(rename = "nutrientId")]
    pub nutrient_id: String,
    pub kind: CoverageKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<(Option<f64>, Option<f64>)>,
    #[serde(rename = "inRange", skip_serializing_if = "Option::is_none")]
    pub in_range: Option<bool>,
    #[serde(rename = "energyPct", skip_serializing_if = "Option::is_none")]
    pub energy_pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Coverage {
    #[serde(rename = "profileId")]
    pub profile_id: String,
    #[serde(rename = "profileDemographic")]
    pub profile_demographic: String,
    #[serde(rename = "foodId")]
    pub food_id: String,
    #[serde(rename = "foodName")]
    pub food_name: String,
    pub coverage: Vec<CoverageEntry>,
    #[serde(rename = "missingNutrients")]
    pub missing_nutrients: Vec<String>,
}

pub fn compute(food: &Food, profile: &RdaProfile, _nutrients: &[Nutrient]) -> Coverage {
    // Index constituents by nutrientId for O(1) lookup.
    let by_nid: HashMap<&str, &FoodConstituent> = food
        .constituents
        .iter()
        .map(|c| (c.nutrient_id.as_str(), c))
        .collect();

    // Deterministic output order.
    let mut nutrient_ids: Vec<&String> = profile.recommendations.keys().collect();
    nutrient_ids.sort();

    // Food's total energy in kcal — in the full foods schema, `calories` is the
    // kcal-valued sibling of `energy` (kJ).
    let total_kcal = food.calories.as_ref().and_then(|c| c.quantity);

    let mut coverage = Vec::new();
    let mut missing = Vec::new();

    for nid in nutrient_ids {
        let rec = &profile.recommendations[nid];
        let constituent = by_nid.get(nid.as_str()).copied();
        match compute_entry(nid, rec, constituent, total_kcal) {
            Some(Ok(entry)) => coverage.push(entry),
            Some(Err(_)) | None => missing.push(nid.clone()),
        }
    }

    Coverage {
        profile_id: profile.id.clone(),
        profile_demographic: profile.demographic.clone(),
        food_id: food.food_id.clone(),
        food_name: food.food_name.clone(),
        coverage,
        missing_nutrients: missing,
    }
}

fn compute_entry(
    nid: &str,
    rec: &Recommendation,
    constituent: Option<&FoodConstituent>,
    food_energy_kcal: Option<f64>,
) -> Option<Result<CoverageEntry, ()>> {
    // Average amount — direct ratio in the recommendation's unit.
    if let Some((rec_val, rec_unit)) = &rec.average_amount {
        let Some((food_val, food_unit)) = food_quantity(constituent) else {
            return Some(Err(()));
        };
        let Some(converted) = convert(food_val, &food_unit, rec_unit) else {
            return Some(Err(()));
        };
        let percent = if *rec_val != 0.0 {
            Some(100.0 * converted / *rec_val)
        } else {
            None
        };
        return Some(Ok(CoverageEntry {
            nutrient_id: nid.to_string(),
            kind: CoverageKind::Average,
            amount: Some(converted),
            unit: Some(rec_unit.clone()),
            percent,
            bounds: None,
            in_range: None,
            energy_pct: None,
        }));
    }

    // Min/max amount — emit bounds + in-range.
    if rec.min_amount.is_some() || rec.max_amount.is_some() {
        let Some((food_val, food_unit)) = food_quantity(constituent) else {
            return Some(Err(()));
        };
        let target_unit = rec
            .min_amount
            .as_ref()
            .map(|(_, u)| u.clone())
            .or_else(|| rec.max_amount.as_ref().map(|(_, u)| u.clone()));
        let Some(target_unit) = target_unit else {
            return Some(Err(()));
        };
        let Some(converted) = convert(food_val, &food_unit, &target_unit) else {
            return Some(Err(()));
        };
        let min = rec.min_amount.as_ref().map(|(v, _)| *v);
        let max = rec.max_amount.as_ref().map(|(v, _)| *v);
        let in_range = min.is_none_or(|m| converted >= m) && max.is_none_or(|m| converted <= m);
        return Some(Ok(CoverageEntry {
            nutrient_id: nid.to_string(),
            kind: CoverageKind::MinMax,
            amount: Some(converted),
            unit: Some(target_unit),
            percent: None,
            bounds: Some((min, max)),
            in_range: Some(in_range),
            energy_pct: None,
        }));
    }

    // Energy share — % of food's total kcal. Uses Atwater kcal-per-gram.
    if rec.min_energy_pct.is_some()
        || rec.max_energy_pct.is_some()
        || rec.average_energy_pct.is_some()
    {
        let Some(factor) = energy_factor(nid) else {
            return Some(Err(()));
        };
        let Some(total_kcal) = food_energy_kcal else {
            return Some(Err(()));
        };
        if total_kcal == 0.0 {
            return Some(Err(()));
        }
        let Some((food_val, food_unit)) = food_quantity(constituent) else {
            return Some(Err(()));
        };
        let Some(grams) = to_base_grams(food_val, &food_unit) else {
            return Some(Err(()));
        };
        let nutrient_kcal = grams * factor;
        let energy_pct = 100.0 * nutrient_kcal / total_kcal;
        let min = rec.min_energy_pct;
        let max = rec.max_energy_pct;
        let average = rec.average_energy_pct;
        let in_range = if min.is_some() || max.is_some() {
            Some(min.is_none_or(|m| energy_pct >= m) && max.is_none_or(|m| energy_pct <= m))
        } else {
            None
        };
        let percent = average.and_then(|a| {
            if a != 0.0 {
                Some(100.0 * energy_pct / a)
            } else {
                None
            }
        });
        return Some(Ok(CoverageEntry {
            nutrient_id: nid.to_string(),
            kind: CoverageKind::Energy,
            amount: None,
            unit: None,
            percent,
            bounds: if min.is_some() || max.is_some() {
                Some((min, max))
            } else {
                None
            },
            in_range,
            energy_pct: Some(energy_pct),
        }));
    }

    None
}

fn food_quantity(c: Option<&FoodConstituent>) -> Option<(f64, String)> {
    let c = c?;
    let q = c.quantity?;
    let u = c.unit.clone()?;
    Some((q, u))
}

fn convert(value: f64, from: &str, to: &str) -> Option<f64> {
    if eq_unit(from, to) {
        return Some(value);
    }
    if let (Some(grams), Some(to_grams_per_unit)) = (to_base_grams(value, from), grams_per_unit(to))
    {
        return Some(grams / to_grams_per_unit);
    }
    None
}

fn eq_unit(a: &str, b: &str) -> bool {
    canonical_unit(a) == canonical_unit(b)
}

fn canonical_unit(u: &str) -> String {
    // U+00B5 MICRO SIGN and U+03BC GREEK SMALL LETTER MU are visually
    // identical and both appear upstream. Fold to one.
    u.replace('\u{03BC}', "\u{00B5}")
}

fn to_base_grams(value: f64, unit: &str) -> Option<f64> {
    grams_per_unit(unit).map(|g| value * g)
}

fn grams_per_unit(unit: &str) -> Option<f64> {
    match canonical_unit(unit).as_str() {
        "g" => Some(1.0),
        "mg" => Some(1e-3),
        "\u{00B5}g" | "ug" => Some(1e-6),
        _ => None,
    }
}

/// Approximate kcal-per-gram factors (Atwater generals). Real nutrition
/// labelling uses per-food specific factors; these stand-ins match what the
/// upstream matvaretabellen UI displays.
fn energy_factor(nutrient_id: &str) -> Option<f64> {
    match nutrient_id {
        "Fett" | "Mettet" | "Trans" | "Flerum" | "Enumet" | "Omega-3" | "Omega-6" => Some(9.0),
        "Alko" => Some(7.0),
        "Karbo" | "Sukker" | "Mono+Di" | "Stivel" | "SUGAN" => Some(4.0),
        "Protein" => Some(4.0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FoodsEnvelope, RdaEnvelope, RdaProfile};

    const NB_FOODS_JSON: &str = include_str!("../data/nb/foods.json");
    const NB_RDA_JSON: &str = include_str!("../data/nb/rda.json");

    fn load_foods() -> Vec<Food> {
        serde_json::from_str::<FoodsEnvelope>(NB_FOODS_JSON)
            .expect("foods.json parses")
            .foods
    }

    fn load_profiles() -> Vec<RdaProfile> {
        serde_json::from_str::<RdaEnvelope>(NB_RDA_JSON)
            .expect("rda.json parses")
            .profiles
    }

    fn first_food_by_id<'a>(foods: &'a [Food], id: &str) -> &'a Food {
        foods
            .iter()
            .find(|f| f.food_id == id)
            .expect("food present")
    }

    #[test]
    fn adzukibonner_coverage_has_average_and_minmax() {
        let foods = load_foods();
        let profiles = load_profiles();
        let food = first_food_by_id(&foods, "06.178");
        let profile = &profiles[0];

        let cov = compute(food, profile, &[]);
        assert!(!cov.coverage.is_empty());

        let avg = cov
            .coverage
            .iter()
            .find(|e| matches!(e.kind, CoverageKind::Average))
            .expect("at least one average entry");
        assert!(avg.percent.unwrap().is_finite());

        let mm = cov
            .coverage
            .iter()
            .find(|e| matches!(e.kind, CoverageKind::MinMax))
            .expect("at least one min/max entry");
        assert!(mm.in_range.is_some());
        assert!(mm.bounds.is_some());
    }

    #[test]
    fn metadata_echoed() {
        let foods = load_foods();
        let profiles = load_profiles();
        let food = first_food_by_id(&foods, "06.178");
        let profile = &profiles[0];

        let cov = compute(food, profile, &[]);
        assert_eq!(cov.food_id, "06.178");
        assert!(cov.food_name.contains("Adzuki"));
        assert_eq!(cov.profile_id, profile.id);
        assert_eq!(cov.profile_demographic, profile.demographic);
    }

    #[test]
    fn energy_kind_emitted_for_fett() {
        let foods = load_foods();
        let profiles = load_profiles();
        let food = first_food_by_id(&foods, "06.178");
        let profile = &profiles[0];

        let cov = compute(food, profile, &[]);
        let fett = cov
            .coverage
            .iter()
            .find(|e| e.nutrient_id == "Fett")
            .expect("Fett recommendation should produce an entry");
        assert!(matches!(fett.kind, CoverageKind::Energy));
        assert!(fett.energy_pct.is_some());
        assert!(fett.in_range.is_some());
    }

    #[test]
    fn unknown_unit_is_reported_missing_without_panic() {
        use crate::types::{Food, FoodConstituent};

        let profiles = load_profiles();
        let profile = &profiles[0];

        let food = Food {
            food_id: "x.x".into(),
            food_group_id: "0".into(),
            food_name: "Synthetic".into(),
            latin_name: None,
            uri: None,
            search_keywords: vec![],
            energy: None,
            calories: None,
            edible_part: None,
            portions: vec![],
            langual_codes: vec![],
            constituents: vec![FoodConstituent {
                nutrient_id: "Fe".into(),
                source_id: Some("test".into()),
                quantity: Some(1.0),
                unit: Some("bogus-unit".into()),
            }],
        };

        let cov = compute(&food, profile, &[]);
        assert!(cov.missing_nutrients.iter().any(|n| n == "Fe"));
        assert!(!cov.coverage.iter().any(|e| e.nutrient_id == "Fe"));
    }

    #[test]
    fn missing_constituent_is_reported_missing() {
        let profiles = load_profiles();
        let profile = &profiles[0];
        let food = Food {
            food_id: "y.y".into(),
            food_group_id: "0".into(),
            food_name: "Empty".into(),
            latin_name: None,
            uri: None,
            search_keywords: vec![],
            energy: None,
            calories: None,
            edible_part: None,
            portions: vec![],
            langual_codes: vec![],
            constituents: vec![],
        };
        let cov = compute(&food, profile, &[]);
        assert!(cov.coverage.is_empty());
        assert_eq!(cov.missing_nutrients.len(), profile.recommendations.len());
    }
}
