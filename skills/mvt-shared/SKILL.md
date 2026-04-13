---
name: mvt-shared
description: Runtime contract for the mvt CLI — output format, error handling, locale semantics, and the full list of embedded resources. Use as foundation before any other mvt skill.
compatibility: Requires the mvt binary installed (install via `curl -fsSL https://raw.githubusercontent.com/alfredvc/matvaretabellen-cli/main/install.sh | sh`). No network required at runtime.
---

# mvt-shared

Foundation skill for the `mvt` CLI — a local wrapper over the Norwegian Food Composition Table (matvaretabellen.no). All data is embedded in the binary. No HTTP calls at runtime (except `mvt update`).

## Output contract

- Success → JSON to stdout, exit 0. Always JSON.
- Error → `{"error": "<message>"}` to stderr, exit 1.
- Use `--json` to force compact JSON if piping into another tool.
- Use `--fields` to slim output. Comma-separated dotted paths: `--fields foodId,foodName,constituents.nutrientId`.

## Locale

`--locale en` (default) or `--locale nb`. Override via `$MVT_LOCALE`.

Locale is strict: a food present in `nb` but not in `en` is a 404, not a silent fallback. If you want either-or, try both locales.

`langual` is language-independent — `--locale` has no effect.

## Resources (all IDs are strings)

| Resource | ID | Notes |
|---|---|---|
| `foods` | `foodId` (e.g. `06.178`) | ~2100 records. Includes constituents, portions, langualCodes, latinName, energy, calories. |
| `food-groups` | `foodGroupId` (e.g. `1.4.5`) | Hierarchical (parentId). |
| `nutrients` | `nutrientId` (e.g. `Fe`, `Vann`) | Has EuroFIR mappings. |
| `sources` | `sourceId` (e.g. `104b`) | Textual description of a data source. |
| `langual` | `langualCode` (e.g. `A0001`) | LanguaL thesaurus. |
| `rda` | `id` (e.g. `rda-1179869501`) | Per-demographic recommendations. First profile = "Generell 18–70 år". |

## Error handling

When exit code is 1, parse stderr as JSON and inspect `.error`. Don't retry on errors like "not found" — they're deterministic.

## Data freshness

```bash
mvt describe | jq .dataVersion            # ISO date at build time
mvt describe --check-upstream | jq .upstreamDrift   # HEAD probes every endpoint
mvt update                                # fetch a newer binary if stale
```

Updates cadence is approximately annual (driven by Mattilsynet releases).

## Read next

- `mvt-food-lookup` — workflow for looking up food nutrient content, browsing food groups, computing RDA coverage, mapping LanguaL codes.
