use crate::data;
use crate::output::{self, Output};
use crate::types::Locale;
use anyhow::Result;
use serde::Serialize;

#[derive(Serialize)]
struct ResourceInfo {
    name: &'static str,
    #[serde(rename = "idField")]
    id_field: &'static str,
    locale: &'static str,
    #[serde(rename = "countNb", skip_serializing_if = "Option::is_none")]
    count_nb: Option<usize>,
    #[serde(rename = "countEn", skip_serializing_if = "Option::is_none")]
    count_en: Option<usize>,
    #[serde(rename = "count", skip_serializing_if = "Option::is_none")]
    count: Option<usize>,
}

#[derive(Serialize)]
struct UpstreamDrift {
    resource: &'static str,
    locale: Option<&'static str>,
    #[serde(rename = "upstreamLastModified")]
    upstream_last_modified: Option<String>,
    #[serde(rename = "embeddedVersion")]
    embedded_version: &'static str,
    stale: Option<bool>,
    error: Option<String>,
}

#[derive(Serialize)]
struct DescribeOutput<'a> {
    #[serde(rename = "cliVersion")]
    cli_version: &'a str,
    #[serde(rename = "dataVersion")]
    data_version: &'static str,
    locales: Vec<&'static str>,
    resources: Vec<ResourceInfo>,
    #[serde(rename = "upstreamDrift", skip_serializing_if = "Option::is_none")]
    upstream_drift: Option<Vec<UpstreamDrift>>,
}

pub fn run(check_upstream: bool, out: &Output) -> Result<()> {
    let resources = vec![
        ResourceInfo {
            name: "foods",
            id_field: "foodId",
            locale: "both",
            count_nb: Some(data::foods(Locale::Nb).items.len()),
            count_en: Some(data::foods(Locale::En).items.len()),
            count: None,
        },
        ResourceInfo {
            name: "foodGroups",
            id_field: "foodGroupId",
            locale: "both",
            count_nb: Some(data::food_groups(Locale::Nb).items.len()),
            count_en: Some(data::food_groups(Locale::En).items.len()),
            count: None,
        },
        ResourceInfo {
            name: "nutrients",
            id_field: "nutrientId",
            locale: "both",
            count_nb: Some(data::nutrients(Locale::Nb).items.len()),
            count_en: Some(data::nutrients(Locale::En).items.len()),
            count: None,
        },
        ResourceInfo {
            name: "sources",
            id_field: "sourceId",
            locale: "both",
            count_nb: Some(data::sources(Locale::Nb).items.len()),
            count_en: Some(data::sources(Locale::En).items.len()),
            count: None,
        },
        ResourceInfo {
            name: "rda",
            id_field: "id",
            locale: "both",
            count_nb: Some(data::rda(Locale::Nb).items.len()),
            count_en: Some(data::rda(Locale::En).items.len()),
            count: None,
        },
        ResourceInfo {
            name: "langual",
            id_field: "langualCode",
            locale: "language-independent",
            count_nb: None,
            count_en: None,
            count: Some(data::langual().items.len()),
        },
    ];

    let upstream_drift = if check_upstream {
        Some(probe_upstream())
    } else {
        None
    };

    let payload = DescribeOutput {
        cli_version: env!("CARGO_PKG_VERSION"),
        data_version: data::DATA_VERSION,
        locales: vec!["en", "nb"],
        resources,
        upstream_drift,
    };

    output::emit(out, serde_json::to_value(payload)?)
}

/// Issue HEAD requests to every endpoint; compare `Last-Modified` with the
/// embedded `DATA_VERSION` (ISO date at refresh time).
fn probe_upstream() -> Vec<UpstreamDrift> {
    let base = std::env::var("MVT_UPSTREAM_BASE_URL")
        .unwrap_or_else(|_| "https://www.matvaretabellen.no/api".into());
    let client = match reqwest::blocking::Client::builder()
        .user_agent("mvt-describe")
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return vec![UpstreamDrift {
                resource: "-",
                locale: None,
                upstream_last_modified: None,
                embedded_version: data::DATA_VERSION,
                stale: None,
                error: Some(format!("failed to build HTTP client: {e}")),
            }]
        }
    };

    let endpoints: &[(&str, Option<&str>, &str)] = &[
        ("foods", Some("nb"), "/nb/foods.json"),
        ("foods", Some("en"), "/en/foods.json"),
        ("foodGroups", Some("nb"), "/nb/food-groups.json"),
        ("foodGroups", Some("en"), "/en/food-groups.json"),
        ("nutrients", Some("nb"), "/nb/nutrients.json"),
        ("nutrients", Some("en"), "/en/nutrients.json"),
        ("sources", Some("nb"), "/nb/sources.json"),
        ("sources", Some("en"), "/en/sources.json"),
        ("rda", Some("nb"), "/nb/rda.json"),
        ("rda", Some("en"), "/en/rda.json"),
        ("langual", None, "/langual.json"),
    ];

    let mut drifts = Vec::new();
    for (name, locale, path) in endpoints {
        let url = format!("{}{}", base.trim_end_matches('/'), path);
        let (last_mod, err) = match client.head(&url).send() {
            Ok(resp) => (
                resp.headers()
                    .get("last-modified")
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.to_string()),
                None,
            ),
            Err(e) => (None, Some(e.to_string())),
        };
        // Best-effort parse of RFC-1123 -> ISO date; on failure leave the raw
        // string in place so the caller still gets a useful comparison.
        let iso = last_mod.as_deref().and_then(parse_http_date_iso);
        let stale = iso.as_deref().map(|d| d != data::DATA_VERSION);
        drifts.push(UpstreamDrift {
            resource: name,
            locale: *locale,
            upstream_last_modified: last_mod,
            embedded_version: data::DATA_VERSION,
            stale,
            error: err,
        });
    }
    drifts
}

fn parse_http_date_iso(s: &str) -> Option<String> {
    // "Sun, 12 Apr 2026 04:08:02 GMT" -> "2026-04-12". No chrono dependency
    // for one format — just split.
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }
    let day = parts[1];
    let mon = parts[2];
    let year = parts[3];
    let mon_num = match mon {
        "Jan" => "01",
        "Feb" => "02",
        "Mar" => "03",
        "Apr" => "04",
        "May" => "05",
        "Jun" => "06",
        "Jul" => "07",
        "Aug" => "08",
        "Sep" => "09",
        "Oct" => "10",
        "Nov" => "11",
        "Dec" => "12",
        _ => return None,
    };
    let day_padded = if day.len() == 1 {
        format!("0{day}")
    } else {
        day.to_string()
    };
    Some(format!("{year}-{mon_num}-{day_padded}"))
}
