# Plan: `mvt` — Matvaretabellen CLI

## Context

The Norwegian Food Composition Table (`matvaretabellen.no`, run by Mattilsynet) publishes the full dataset as a small set of bulk JSON endpoints at `https://www.matvaretabellen.no/api/`. There is **no REST API** for search or per-item fetch — the site itself is a statically-built ClojureScript app that downloads bulk JSON and tokenizes client-side ([github.com/Mattilsynet/matvaretabellen-deux](https://github.com/Mattilsynet/matvaretabellen-deux)).

We want an agent-friendly Rust CLI (`mvt`) that wraps this data and is usable offline, in sandboxes, and in CI without network. The dataset updates roughly annually. zstd-19 compresses the entire dataset (both locales, all 7 resources) to ~1.35 MB — trivial to embed in the binary.

**Intended outcome:** single-binary CLI that serves the whole dataset locally, with JSON-to-stdout contract designed for agent consumption. New data releases ship via new CLI releases through GitHub Releases + `mvt update` self-update.

## Scope

### In scope (v1)

- Six resources: `foods`, `food-groups`, `nutrients`, `sources`, `langual`, `rda`. We drop the undocumented `compact-foods` endpoint — the documented `foods.json` carries `searchKeywords` natively and its `constituents` array is fully typeable (each element has a `nutrientId`, unlike the compact version's polymorphic map). Single source of truth simplifies the data layer and drops ~430 KB from the binary.
- Two locales: `nb` (source-of-truth), `en`. CLI default: `en` for agents.
- Commands: `list`, `get <id>`; for foods also `search <query>` and `rda <foodId> [--profile <id>]`.
- Global flags: `--json` (force compact), `--fields f1,f2`, `--locale en|nb`.
- Output contract: JSON to stdout, exit 0 on success; `{"error": "..."}` to stderr, exit 1 on error; pretty-print when TTY, compact when piped.
- Strongly-typed structs for every resource (see "Typing strategy" below).
- `mvt describe [--check-upstream]` — schema summary, data version, resource counts; `--check-upstream` HEADs upstream and reports drift.
- `mvt update` — self-update from GitHub releases (brings new embedded data + new code).
- Real upstream-parity search: port the tokenizer pipeline from [matvaretabellen-deux](https://github.com/Mattilsynet/matvaretabellen-deux/blob/main/src/matvaretabellen/search.cljc) — edge n-grams, diacritics folding, stopword filter per locale. Index built from `foodName` + `searchKeywords` (already compound-split server-side).
- `mvt foods rda <foodId> [--profile <id>]` — emits per-nutrient RDA coverage for the food, using the chosen profile (default: first profile in `rda.json`, which is `Generell 18–70 år`). Output: `{profile, food, coverage: [{nutrientId, amount, unit, recommendation, percent, kind}]}`.
- Embedded data: 11 JSON files (`{nb,en}` × 5 resources + `langual`) compressed with zstd-19 at build time. Resources are parsed into typed structs once per process behind `OnceLock`.
- Locale fallback semantics: `get --locale en` on a food not present in `en` returns `{"error": "not found in locale en"}` exit 1 — **no silent fallback to nb**. Explicit caller choice.

### Deferred (v2+)

- EDN passthrough output (`--format edn`) — low demand, agents prefer JSON.
- Fuzzy matching beyond edge-ngrams (e.g. Levenshtein) — ngrams+stopwords covers the bulk.
- Standalone `mvt update --data-only` — self-update covers it; noted as limitation.
- Config file. No auth, no per-user config in v1. Locale flag + env var sufficient.
- Persistent decompressed cache in `~/.cache/mvt/` — reassess if repeated-invocation benchmarks show a problem.

### Known limitations (explicit — doc'd in SKILL.md)

- Data freshness tied to release cadence. User runs `mvt update` to get new data. `mvt describe --check-upstream` flags drift.
- No interactive auth. None needed.
- Release binary: 4.7 MB (measured). Baseline Rust + ~1 MB zstd-compressed data.
- `--locale en` is strict: a record absent from the English export fails rather than falls back. Callers who want either-or must try both locales.

## API research findings

**Base URL:** `https://www.matvaretabellen.no/api/`

**Authentication:** none.

**ID types (all strings):**

| Resource | ID field (JSON) | Example |
|---|---|---|
| foods | `foodId` | `06.178` |
| food-groups | `foodGroupId` | `1`, `1.4.5` |
| nutrients | `nutrientId` | `Vann`, `Vit E`, `C18:1` |
| sources | `sourceId` | `0`, `104b`, `MI0115` |
| langual | `langualCode` | `A0001` |
| rda profiles | `id` | `rda-1179869501` |

**Endpoints (all GET, JSON):**

```
/api/{nb,en}/foods.json           13 MB   {foods: [...], locale: "nb"}
/api/{nb,en}/food-groups.json     6 KB    {foodGroups: [...]}
/api/{nb,en}/nutrients.json       13 KB   {nutrients: [...]}
/api/{nb,en}/sources.json         95 KB   {sources: [...]}
/api/{nb,en}/compact-foods.json   4.7 MB  [ {...}, ... ]        (undocumented)
/api/{nb,en}/rda.json             35 KB   {profiles: [...]}     (undocumented)
/api/langual.json                 426 KB  {codes: [...]}        (language-independent)
```

Response envelopes vary by resource — list commands unwrap the known key (`foods`, `foodGroups`, etc.) and emit a flat array. `compact-foods` is already a flat array. `langual` has no locale.

**Error format:** n/a at runtime (no network calls). CLI-generated JSON errors only.

## Command structure

```
mvt
├── --json                      (global, force compact JSON)
├── --fields f1,f2              (global, filter output fields; dotted paths supported)
├── --locale {en|nb}            (global, default: en, or $MVT_LOCALE)
│
├── foods
│   ├── list                    (compact records; use --full for full records)
│   ├── get <foodId> [--full]   (default compact; --full = full record with constituents/portions/etc.)
│   ├── search <query>          (edge-ngram + diacritic-folded tokenizer, compound-aware via upstream searchKeywords)
│   └── rda <foodId> [--profile <id>]  (per-nutrient RDA coverage for the food)
├── food-groups
│   ├── list
│   └── get <foodGroupId>
├── nutrients
│   ├── list
│   └── get <nutrientId>
├── sources
│   ├── list
│   └── get <sourceId>
├── langual
│   ├── list                    (no --locale flag effect)
│   └── get <langualCode>
├── rda
│   ├── list                    (profile summaries)
│   └── get <profileId>
├── describe [--check-upstream] (embedded schema + data version + counts; optionally HEAD upstream to report drift)
└── update                      (self-update from GitHub releases)
```

## File structure

```
matvaretabellen-cli/
├── Cargo.toml
├── README.md
├── AGENTS.md            (symlink -> CLAUDE.md)
├── build.rs             (compresses data/*.json -> OUT_DIR/*.json.zst)
├── data/                (committed raw JSON fixtures; regenerated by scripts/refresh-data.sh)
│   ├── nb/foods.json           (13 MB)
│   ├── nb/compact-foods.json
│   ├── nb/food-groups.json
│   ├── nb/nutrients.json
│   ├── nb/sources.json
│   ├── nb/rda.json
│   ├── en/foods.json
│   ├── en/compact-foods.json
│   ├── en/food-groups.json
│   ├── en/nutrients.json
│   ├── en/sources.json
│   ├── en/rda.json
│   ├── langual.json
│   └── VERSION                (ISO date from upstream Last-Modified; shown by `mvt describe`)
├── scripts/
│   └── refresh-data.sh         (curl all 13 upstream files into data/)
├── src/
│   ├── main.rs                 (clap dispatch, error->stderr, exit code; no #[tokio::main])
│   ├── cli.rs                  (all clap derive structs + global flags)
│   ├── data.rs                 (include_bytes! + zstd decompress + OnceLock cache, typed + indexed)
│   ├── types.rs                (typed structs: CompactFood, FoodGroup, Nutrient, Source, LangualCode, RdaProfile)
│   ├── search.rs               (tokenizer, edge-ngram index, scoring; port of upstream search.cljc)
│   ├── rda.rs                  (RDA coverage calculator: nutrient quantity -> %recommendation)
│   ├── output.rs               (JSON format, TTY detect, --fields filter, dotted paths)
│   ├── update.rs               (GitHub releases self-update, atomic rename; builds its own tokio Runtime)
│   └── commands/
│       ├── mod.rs
│       ├── foods.rs            (list / get / search / rda subcommands)
│       ├── food_groups.rs
│       ├── nutrients.rs
│       ├── sources.rs
│       ├── langual.rs
│       ├── rda.rs              (list / get profiles)
│       ├── describe.rs
│       └── update.rs
├── tests/
│   ├── foods.rs
│   ├── food_groups.rs
│   ├── nutrients.rs
│   ├── sources.rs
│   ├── langual.rs
│   ├── rda.rs
│   ├── describe.rs
│   ├── output_filters.rs
│   └── search.rs
├── skills/
│   ├── mvt-shared/SKILL.md
│   └── mvt-food-lookup/SKILL.md
├── .github/workflows/
│   ├── ci.yml                  (fmt + clippy + test)
│   └── release.yml             (cross-platform release, patches Cargo.toml version from tag)
├── install.sh
└── .git/hooks/pre-commit       (cargo fmt --check && cargo test; doc'd in README)
```

## Typing strategy

Every flat-schema resource is a strongly-typed `#[derive(Deserialize, Serialize)]` struct in `src/types.rs` with `#[serde(rename_all = "camelCase")]` to match upstream. This catches schema drift at parse time rather than silently producing empty results downstream.

| Resource | Type | Reason |
|---|---|---|
| `compact-foods` | `Vec<CompactFood>` | Hot path (search, list). Fields: `id`, `food_name`, `food_group_id`, `url`, `energy_kj`, `energy_kcal`, `edible_part`, `search_keywords: Vec<String>`, `constituents: HashMap<String, Constituent>`. |
| `food-groups` | `Vec<FoodGroup>` | Flat: `id`, `name`, `parent_id: Option<String>`. |
| `nutrients` | `Vec<Nutrient>` | Flat: `id`, `name`, `unit`, `decimal_precision`, `euro_fir_id`, `euro_fir_name`, `parent_id: Option<String>`, `uri`. |
| `sources` | `Vec<Source>` | Flat: `id`, `description`. |
| `langual` | `Vec<LangualCode>` | Flat: `id` (wire name `langualCode`), `description`. |
| `rda` | `Vec<RdaProfile>` | `id`, `demographic`, `energy_recommendation: [f64; 2]` as `(value, unit)`, `kcal_recommendation`, `recommendations: HashMap<String, Recommendation>` where `Recommendation` is an enum covering `average_amount`, `min_amount`, `max_amount`, `min_energy_pct`, `max_energy_pct`. |
| `foods` (full) | `serde_json::Value` | **Exception.** `constituents` is keyed by arbitrary nutrient IDs (`Vann`, `C18:1`, `Vit E`, …) — modelling this precisely is disproportionate work and `--fields constituents.Fett.quantity` works trivially on `Value`. |

A per-locale `HashMap<String, usize>` index is built alongside each typed `Vec<T>` (both behind a single `OnceLock<(Vec<T>, HashMap<String, usize>)>`), so `get <id>` is O(1) by string key. Indices, not references, to avoid lifetime gymnastics.

## Data embedding

### Build time (`build.rs`)

- For each file in `data/`, compress with zstd-19 into `$OUT_DIR/<path>.zst`. Emit a `DATA_VERSION` constant from `data/VERSION`.
- Emit `UPSTREAM_ETAGS` constant — a map from `{locale, resource}` to the `ETag` captured at refresh time. Used by `describe --check-upstream`.
- `cargo:rerun-if-changed=data/`.

### Run time (`src/data.rs`)

- `include_bytes!($OUT_DIR/nb/foods.json.zst)` etc. for each file.
- On first access per `(locale, resource)`: `zstd::decode_all` → `serde_json::from_slice::<T>` (or `Value` for full foods) → build id-index → store in `OnceLock`.
- Compact and full foods are **separate embeds** — `list` / `search` / compact `get` never decompress the 13 MB full file.
- `zstd` crate added with `default-features = false` to avoid transitively pulling experimental features.

**Why not decompress all at startup:** `mvt food-groups list` should stay under ~10 ms. Lazy decompression keeps cold commands fast; only `foods get --full` pays the ~50 ms cost of decoding the 13 MB full foods payload.

## Output contract

- Success → JSON to stdout, exit 0. Pretty-printed iff stdout is a TTY and `--json` not set.
- Error → `{"error": "<message>"}` to stderr, exit 1.
- `--fields a,b,c.d` filters top-level fields on objects; for arrays, filters each element; supports dotted paths (`energy.quantity`) for nested fields.
- `list` emits a JSON array. No pagination envelope — the whole list fits.
- `get` emits a single JSON object, or `{"error": "not found"}` exit 1.
- `search` emits a JSON array of compact records, ordered by match score (exact-token > prefix > substring).

## Tech stack

| Component | Crate | Rationale |
|---|---|---|
| CLI parsing | `clap` 4 (derive) | Type-safe args, auto help |
| Async runtime | `tokio` | Required by `reqwest` in `update` only |
| HTTP (update only) | `reqwest` (rustls-tls, no openssl) | Self-update from GitHub Releases |
| JSON | `serde`, `serde_json` | Standard |
| Compression | `zstd` | Best ratio for JSON; widely used |
| Error handling | `anyhow` | Context-rich errors |
| Build | `zstd` (build-dep) | Precompress at build time |
| Self-update | `semver`, `flate2`, `tar`, `tempfile` | Per cli-creator template |
| Testing | `assert_cmd`, `predicates`, `tempfile` | Subprocess CLI tests |

No `reqwest` / `tokio` runtime entered unless `update` subcommand runs — gated behind `if let Commands::Update { .. }`.

## Implementation order

1. **Scaffold.** `cargo init --bin`, Cargo.toml deps, data dir + VERSION. Commit.
2. **`scripts/refresh-data.sh`.** Pulls all 13 JSON files from upstream into `data/`, writes `data/VERSION` from the most recent `Last-Modified`. Runs idempotently; CI will use it on demand.
3. **`build.rs`.** Precompresses `data/**/*.json` to `$OUT_DIR/**/*.json.zst`. Emits `pub const DATA_VERSION: &str`.
4. **`src/data.rs`.** `include_bytes!` + `zstd::decode_all` + `OnceLock` cache keyed by `(Locale, Resource)`. Each resource returns `&'static serde_json::Value`.
5. **`src/output.rs`.** `Output` struct, TTY detect (`std::io::IsTerminal`), `--fields` filter with dotted paths. Unit tests.
6. **`src/cli.rs`.** All clap structs: `Cli`, `Commands`, subcommand enums per resource.
7. **List/get commands per resource** — foods first (both compact & full variants), then food-groups, nutrients, sources, langual, rda.
8. **`src/search.rs`.** Case-insensitive tokenizer, match scorer, ordering. Unit tests against known fixtures.
9. **`foods search` command.**
10. **`describe` command.** Emits the shape below. `--check-upstream` adds a `upstreamDrift: [...]` field by issuing HEAD requests to each endpoint and comparing `ETag` / `Last-Modified` with build-time values.
    ```json
    {
      "cliVersion": "0.1.0",
      "dataVersion": "2026-04-12",
      "locales": ["en", "nb"],
      "resources": [
        {"name": "foods", "count": 2121, "locale": "both", "idField": "foodId", "embedded": "full+compact"},
        {"name": "foodGroups", "count": 48, "locale": "both", "idField": "foodGroupId"},
        {"name": "nutrients", "count": 73, "locale": "both", "idField": "nutrientId"},
        {"name": "sources", "count": 81, "locale": "both", "idField": "sourceId"},
        {"name": "langual", "count": 4350, "locale": "language-independent", "idField": "langualCode"},
        {"name": "rda", "count": 8, "locale": "both", "idField": "id"}
      ],
      "upstreamDrift": null
    }
    ```
    With `--check-upstream` on a drift: `"upstreamDrift": [{"resource": "foods", "locale": "nb", "embedded_etag": "...", "upstream_etag": "...", "stale": true}]`.
11. **`src/update.rs` + `update` command.** Per cli-creator template: detect cargo-installed binary, download from GitHub Releases, atomic rename. Gate behind env var overrides for test injection (`MVT_GITHUB_API_URL`, `MVT_GITHUB_DOWNLOAD_URL`, `MVT_SELF_PATH`, `MVT_CURRENT_VERSION`, `MVT_SKIP_CARGO_CHECK`).
12. **Integration tests** per command.
13. **Pre-commit hook.** `.git/hooks/pre-commit` runs `cargo fmt --check && cargo test`. Document in README.
14. **CI workflows** (`ci.yml`, `release.yml`). Release workflow patches Cargo.toml version from tag.
15. **`install.sh`** with `curl -f`, default to `~/.local/bin`.
16. **Docs.** README + AGENTS.md + skills/.

## Key implementation patterns

### Embedded-data dispatch (data.rs)

```rust
#[derive(Copy, Clone)]
pub enum Locale { Nb, En }

// One block per resource via macro. Sketch for typed resources:
struct CompactFoodsCache { items: Vec<CompactFood>, by_id: HashMap<String, usize> }
static COMPACT_FOODS_NB: OnceLock<CompactFoodsCache> = OnceLock::new();
static COMPACT_FOODS_EN: OnceLock<CompactFoodsCache> = OnceLock::new();

pub fn compact_foods(locale: Locale) -> &'static CompactFoodsCache {
    let cell = match locale { Locale::Nb => &COMPACT_FOODS_NB, Locale::En => &COMPACT_FOODS_EN };
    cell.get_or_init(|| {
        let bytes: &[u8] = match locale {
            Locale::Nb => include_bytes!(concat!(env!("OUT_DIR"), "/nb/compact-foods.json.zst")),
            Locale::En => include_bytes!(concat!(env!("OUT_DIR"), "/en/compact-foods.json.zst")),
        };
        let decoded = zstd::decode_all(bytes).expect("embedded data corrupt");
        let items: Vec<CompactFood> = serde_json::from_slice(&decoded)
            .expect("embedded compact-foods schema drift");
        let by_id = items.iter().enumerate()
            .map(|(i, f)| (f.id.clone(), i)).collect();
        CompactFoodsCache { items, by_id }
    })
}
```

Full foods keeps `Value` but also builds a `HashMap<String, usize>` into the `foods` array on first access.

### Tokio runtime — gated to `update`

`fn main() -> anyhow::Result<()>` is **synchronous**. Only the `update` subcommand constructs a tokio runtime:

```rust
fn run_update(args: UpdateArgs) -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    rt.block_on(update::run(args))
}
```

This keeps `mvt foods list` from paying tokio startup cost.

### Field filtering with dotted paths (output.rs)

Support `--fields foodName,energy.quantity`. On objects, pick matching top-level keys and recurse into nested selections. On arrays, map over each element. Keep non-selected parents empty; don't fabricate structure. Applied as a post-serialization pass over `serde_json::Value`, so it works uniformly for typed and untyped resources.

### `foods get` compact vs full

Default `get` uses compact typed records. `--full` loads the full `Value` foods file for constituents/portions/langualCodes/latin-name. Both lookup paths use the pre-built `HashMap<String, usize>` index.

### Search (search.rs) — upstream-parity tokenizer

Ported from [`src/matvaretabellen/search.cljc`](https://github.com/Mattilsynet/matvaretabellen-deux/blob/main/src/matvaretabellen/search.cljc). Pipeline:

1. **Index input.** For each compact food, combine `food_name` with all `search_keywords`. Mattilsynet's `searchKeywords` is already compound-split server-side — reuse it, don't re-derive.
2. **Normalize.** Lowercase, fold diacritics (`ø→o`, `å→a`, `é→e`, etc.) so `bønne` and `bonne` both hit.
3. **Stopword filter.** Per-locale stopword list (short: `og`, `i`, `til`, `med` for nb; `and`, `of`, `with`, `the` for en). Source from upstream.
4. **Edge n-grams.** For each surviving token, emit prefixes of length 3..=min(len, 10). Index `token_prefix → set of food indices`.
5. **Query.** Tokenize + normalize + filter the query the same way, then for each query token take the edge-ngram prefix of its first 3 chars and intersect posting sets.
6. **Score.** +10 exact `food_name` match; +5 exact token in `food_name`; +3 exact token in `search_keywords`; +1 per matching ngram in either field.
7. Sort by score desc; break ties by shorter `food_name` (proxy for specificity); emit as array.

Unit tests cover: `"egg"` matches `"Eggerøre"` via compound+ngram; `"bønne"` matches `"Adzukibønner, tørr"`; `"laks"` matches `"Røykelaks"`; stopwords are excluded; diacritics fold. The upstream repo provides example queries/expected results that we'll use as acceptance fixtures.

### RDA coverage (rda.rs)

`mvt foods rda <foodId> [--profile <id>]`:

1. Load compact food by id.
2. Load RDA profile (default: first profile in `rda.json`, which is `rda-1179869501 / Generell 18-70 år`).
3. For each `(nutrientId, recommendation)` in the profile where the food has a constituent quantity:
   - If `recommendation.averageAmount = [x, unit]` — compute `percent = 100 * food_quantity / x` (after unit normalization).
   - If `min_amount`/`max_amount` — emit both bounds and whether the food falls in range.
   - If `min_energy_pct`/`max_energy_pct` — compute the nutrient's energy contribution vs food's total kcal.
4. Skip nutrients without a matching constituent (emit `missing: [nutrientId...]` summary instead).
5. Output shape:

```json
{
  "profile": {"id": "rda-1179869501", "demographic": "..."},
  "food": {"id": "06.178", "foodName": "..."},
  "coverage": [
    {"nutrientId": "Fe", "amount": 5.0, "unit": "mg", "recommendation": {"averageAmount": [10.8, "mg"]}, "percent": 46.3, "kind": "average"},
    {"nutrientId": "Fett", "percent": 1.9, "kind": "energy", "bounds": {"min": 25, "max": 40}, "inRange": false}
  ],
  "missingNutrients": ["VitK1", ...]
}
```

Unit normalization table (g↔mg↔µg) lives in `rda.rs`. No external unit crate — fixed scales.

## Testing strategy

**Layer 1 — unit tests:** output formatting, field filtering (including dotted paths + arrays), search tokenization + scoring, locale parsing.

**Layer 2 — integration tests (`tests/*.rs`):** spawn the actual binary with `assert_cmd`, assert JSON output shape against fixtures derived from real upstream data. No HTTP mocks needed — data is embedded. One test file per command group.

**Critical tests:**
- `foods list --fields id,foodName` — array preserved, items filtered.
- `foods get 06.178` (compact) and `foods get 06.178 --full` — different field sets.
- `foods search "bønne"` (nb), `"bean"` (en), `"egg"` matches `"Eggerøre"` via compound+ngram, `"bonne"` (no diacritic) matches `"bønne"`, stopwords excluded.
- `foods rda 06.178` — `coverage` array non-empty, each entry has `percent` or `inRange`, `missingNutrients` array present.
- `foods rda 06.178 --profile <non-existent>` — exit 1 with error.
- Schema drift canary: a unit test deserializes each embedded file into its typed struct at test time; failure = data refresh broke schema.
- `--fields constituents.Fett.quantity` — dotted-path filter through array inside object (on `--full`).
- Locale-strict: `foods get <id-absent-from-en> --locale en` → exit 1, message mentions locale.
- Not-found paths for every resource — exit 1, stderr JSON error.
- `describe` — reports version from `data/VERSION`, resource counts match.
- `describe --check-upstream` (mocked via wiremock + env-var base URL override) — drift detection works.
- TTY vs piped formatting — use `assert_cmd` + `predicates::str::contains`.

**Layer 3 — refresh + snapshot:** `scripts/refresh-data.sh` pulls fresh upstream JSON; CI workflow runs tests against the committed snapshot, not live upstream. Refresh is a manual pre-release step.

**Update command tests:** use `MVT_GITHUB_API_URL` / `MVT_GITHUB_DOWNLOAD_URL` / `MVT_SELF_PATH` env-var overrides + `wiremock`. Validate: cargo-install detection, download, atomic rename, rate-limit error path.

## Distribution

**GitHub Actions matrix:** x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu (cross), x86_64-apple-darwin, aarch64-apple-darwin.

**Release workflow MUST** patch `Cargo.toml` version from the pushed tag before `cargo build --release`, else binaries report whatever version Cargo.toml currently has (per cli-creator `Common Pitfalls`).

**Artifacts:** `mvt-{os}-{arch}.tar.gz` attached to release.

**Install script:** default target `~/.local/bin`; use `curl -f`; double-check repo slug in script.

## Agent skills

Two skills shipped in `skills/` (installable via `npx skills add <owner>/matvaretabellen-cli`):

- **`mvt-shared/SKILL.md`** — foundation: output contract, exit codes, locale flag, fields filter, resource list, no-network guarantee.
- **`mvt-food-lookup/SKILL.md`** — workflow: looking up a food's nutrient content, listing foods in a group, finding RDA context, mapping LanguaL codes. Each step shows exact command + flags (e.g. `mvt foods search "egg" --fields id,foodName,energyKcal`).

Both SKILL.md files use YAML frontmatter per `cli-creator` spec.

## Pitfall checklist (from cli-creator)

- [x] All IDs are strings (verified against real fixtures). No `u64` parsing.
- [x] Mock data = real fixtures, committed to `data/`.
- [x] Errors → stderr JSON, exit 1. Success → stdout, exit 0.
- [x] `--fields` on list: array preserved, items filtered.
- [x] Release workflow patches version from tag.
- [x] Install script uses `curl -f`, defaults to `~/.local/bin`.
- [x] Pre-commit hook documented.
- [x] Self-update detects cargo-installed binary; atomic rename.
- [x] Test injection env vars for update.
- N/A Pagination (single dump per resource).
- N/A Auth precedence (no auth).
- N/A Accept-header per endpoint (all JSON).

## Verification

End-to-end checks before cutting the first release:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test

# Smoke tests against real binary
cargo build --release
./target/release/mvt foods list --fields id,foodName | jq 'length'            # expect > 2000
./target/release/mvt foods get 06.178 | jq .foodName                           # "Adzuki beans, dry" (default en)
./target/release/mvt foods get 06.178 --locale nb | jq .foodName               # "Adzukibønner, tørr"
./target/release/mvt foods get 06.178 --full | jq '.constituents | length'    # > 40
./target/release/mvt foods search "bean" --fields id,foodName | jq 'length'   # > 0
./target/release/mvt foods search "bønne" --locale nb | jq 'length'           # > 0 (ngram+diacritic)
./target/release/mvt foods search "egg" --locale nb | jq '[.[] | .foodName] | map(contains("gger")) | any'  # true (compound match)
./target/release/mvt foods rda 06.178 | jq '.coverage | length'               # > 0
./target/release/mvt food-groups get 1.4.5 | jq .name                          # Brunost / equiv
./target/release/mvt langual get A0001 | jq .description                       # "Product type, not known"
./target/release/mvt rda list | jq 'length'                                    # >= 1
./target/release/mvt describe | jq '.dataVersion, .resources[0].count'
./target/release/mvt describe --check-upstream | jq '.upstreamDrift'           # null or array

# Error path
./target/release/mvt foods get doesnotexist; echo "exit=$?"                 # exit=1, stderr JSON
```

Binary size check:

```bash
ls -la target/release/mvt    # expect ~6-8 MB
```

If binary > 15 MB or any smoke-test exits non-zero, block release.
