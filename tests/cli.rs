//! End-to-end CLI tests. Spawn the actual `mvt` binary with assert_cmd and
//! assert on JSON output shape.

use assert_cmd::Command;
use serde_json::Value;

fn mvt() -> Command {
    // --json forces compact output so we get deterministic serialized shape.
    let mut c = Command::cargo_bin("mvt").expect("binary built");
    c.arg("--json");
    c
}

fn run(args: &[&str]) -> (Value, Value, i32) {
    let out = mvt().args(args).output().expect("spawn mvt");
    let stdout = if out.stdout.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&out.stdout).unwrap_or(Value::Null)
    };
    let stderr = if out.stderr.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&out.stderr).unwrap_or(Value::Null)
    };
    (stdout, stderr, out.status.code().unwrap_or(-1))
}

#[test]
fn foods_list_has_expected_cardinality() {
    let (v, _, code) = run(&["foods", "list", "--fields", "foodId"]);
    assert_eq!(code, 0);
    let arr = v.as_array().expect("array");
    assert!(arr.len() > 2000, "expected > 2000 foods, got {}", arr.len());
}

#[test]
fn foods_get_default_locale_en() {
    let (v, _, code) = run(&["foods", "get", "06.178"]);
    assert_eq!(code, 0);
    assert_eq!(
        v["foodName"].as_str(),
        Some("Adzuki beans, uncooked"),
        "expected English name, got {v}"
    );
}

#[test]
fn foods_get_nb_locale_has_norwegian_name() {
    let (v, _, code) = run(&["--locale", "nb", "foods", "get", "06.178"]);
    assert_eq!(code, 0);
    let name = v["foodName"].as_str().unwrap_or("");
    assert!(name.contains("Adzukibønner"), "got name: {name}");
}

#[test]
fn foods_get_full_has_constituents() {
    let (v, _, code) = run(&["foods", "get", "06.178"]);
    assert_eq!(code, 0);
    let consts = v["constituents"].as_array().expect("array");
    assert!(
        consts.len() > 40,
        "expected > 40 constituents, got {}",
        consts.len()
    );
}

#[test]
fn foods_search_english() {
    let (v, _, code) = run(&["foods", "search", "bean", "--fields", "foodId,foodName"]);
    assert_eq!(code, 0);
    let arr = v.as_array().expect("array");
    assert!(!arr.is_empty(), "no hits for 'bean'");
}

#[test]
fn foods_search_norwegian_diacritic_folds() {
    let (v, _, _) = run(&["--locale", "nb", "foods", "search", "bonne"]);
    let arr = v.as_array().expect("array");
    assert!(
        !arr.is_empty(),
        "expected hits for 'bonne' (diacritic fold to bønne)"
    );
}

#[test]
fn foods_rda_coverage_has_entries() {
    let (v, _, code) = run(&["foods", "rda", "06.178"]);
    assert_eq!(code, 0);
    let cov = v["coverage"].as_array().expect("coverage array");
    assert!(!cov.is_empty());
    // Deterministic nutrient must appear: Fe as Average with a percent.
    let fe = cov
        .iter()
        .find(|e| e["nutrientId"].as_str() == Some("Fe"))
        .expect("Fe coverage entry");
    assert_eq!(fe["kind"].as_str(), Some("average"));
    assert!(fe["percent"].as_f64().is_some());
}

#[test]
fn food_groups_get_works() {
    let (v, _, code) = run(&["--locale", "nb", "food-groups", "get", "1.4.5"]);
    assert_eq!(code, 0);
    assert_eq!(v["name"].as_str(), Some("Brunost"));
}

#[test]
fn langual_get_works() {
    let (v, _, code) = run(&["langual", "get", "A0001"]);
    assert_eq!(code, 0);
    assert_eq!(v["description"].as_str(), Some("Product type, not known"));
}

#[test]
fn describe_reports_embedded_schema() {
    let (v, _, code) = run(&["describe"]);
    assert_eq!(code, 0);
    assert!(!v["dataVersion"].as_str().unwrap_or("").is_empty());
    assert!(!v["resources"].as_array().unwrap().is_empty());
    // Neither countNb nor countEn should be zero for foods.
    let foods = v["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"].as_str() == Some("foods"))
        .unwrap();
    assert!(foods["countNb"].as_u64().unwrap() > 2000);
    assert!(foods["countEn"].as_u64().unwrap() > 2000);
}

#[test]
fn not_found_exits_nonzero_with_json_error_on_stderr() {
    let (_, err, code) = run(&["foods", "get", "doesnotexist"]);
    assert_eq!(code, 1);
    assert!(
        err["error"].as_str().unwrap_or("").contains("doesnotexist"),
        "stderr: {err}"
    );
}

#[test]
fn fields_filter_preserves_array_shape() {
    let (v, _, code) = run(&["foods", "list", "--fields", "foodId,foodName"]);
    assert_eq!(code, 0);
    let arr = v.as_array().unwrap();
    for item in arr.iter().take(5) {
        let obj = item.as_object().unwrap();
        assert_eq!(obj.len(), 2);
        assert!(obj.contains_key("foodId"));
        assert!(obj.contains_key("foodName"));
    }
}
