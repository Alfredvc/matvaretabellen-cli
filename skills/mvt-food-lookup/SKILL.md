---
name: mvt-food-lookup
description: Step-by-step workflow for looking up a food's nutrient content, finding foods by name or group, computing RDA coverage, and mapping LanguaL codes via the mvt CLI.
compatibility: Requires `mvt-shared` as foundation. Requires the mvt binary installed.
---

# mvt-food-lookup

Workflow-oriented skill for nutrition data lookups. Read `mvt-shared` first.

## Find a food by name

```bash
# English search
mvt foods search "egg" --limit 20 --fields foodId,foodName --json

# Norwegian search — edge-ngrams + diacritic folding, so "bonne" matches "bønne"
mvt --locale nb foods search "bonne" --limit 20 --fields foodId,foodName --json
```

Search scoring: whole-name match > whole-word name hit > whole-word keyword hit > edge-ngram only. Ties broken by shorter name.

## Get full nutrient breakdown for a food

```bash
# All top-level fields
mvt foods get 06.178 --json

# Just constituents (nutrient array)
mvt foods get 06.178 --fields foodId,foodName,constituents --json

# Slice a specific nutrient with jq
mvt foods get 06.178 --json | jq '.constituents[] | select(.nutrientId=="Fe")'
```

Each constituent has `{nutrientId, quantity?, unit?, sourceId?}`. Quantity/unit may be missing (`sourceId` only) — that means "known-unknown", not "zero".

## RDA coverage for a food

```bash
# Default profile: "Generell 18-70 år" (first in list)
mvt foods rda 06.178 --json

# Specific profile
mvt rda list --fields id,demographic --json                # browse
mvt foods rda 06.178 --profile rda-1273171300 --json       # "Man 25-50 years"
```

Coverage entry shapes:
- `kind: "average"` → has `percent` (100 × food amount ÷ recommended).
- `kind: "minmax"` → has `bounds: [min, max]` and `inRange`.
- `kind: "energy"` → has `energyPct` (nutrient's % of food's total kcal) and `bounds`/`inRange` where applicable.

Nutrients the profile recommends but the food doesn't cover (or unknown unit) land in `missingNutrients`.

## Browse food groups

```bash
# Full tree
mvt food-groups list --json

# Drill into a group
mvt food-groups get 1.4 --json         # "Ost" / "Cheese"
mvt food-groups get 1.4.5 --locale nb --json   # "Brunost"

# Find foods in a group (client-side filter)
mvt foods list --fields foodId,foodName,foodGroupId --json \
  | jq '.[] | select(.foodGroupId == "1.4.5")'
```

## Resolve LanguaL codes

A food's `langualCodes` is a list of short codes. Decode:

```bash
mvt foods get 06.178 --fields langualCodes --json
# ["N0001","G0003","A0152",...]

mvt langual get A0152 --json
```

## Tips

- Prefer `--fields` for every list/search that will be read by another tool. The unfiltered `foods list` is ~13 MB of JSON.
- `--json` only forces compact. Defaults already serve JSON — `--json` matters mostly when piping from a TTY.
- Norwegian `searchKeywords` are compound-split server-side (`"eggerøre"` may have a keyword like `"egg"`). Use `--locale nb` for best recall on Norwegian product names.
- If a search returns no hits, try the other locale — some foods only have one language's keywords.
- `mvt describe --check-upstream` is the canonical staleness check. Run it if an answer looks outdated.
