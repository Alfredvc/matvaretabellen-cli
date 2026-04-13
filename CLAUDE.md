# mvt — agent reference

Agent-friendly CLI for the Norwegian Food Composition Table. All data is embedded; no network required at runtime (except `mvt update`).

## Output contract

- Success → JSON to stdout, exit 0. Pretty in a TTY, compact when piped or with `--json`.
- Error → `{"error": "..."}` JSON to stderr, exit 1.
- `mvt --json ...` forces compact JSON regardless of TTY.
- `--fields a,b,c.d` filters fields; dotted paths recurse into objects, arrays are mapped element-wise.
- `--locale {en|nb}` picks dataset locale (default `en`; `langual` is language-independent). `$MVT_LOCALE` also works.

## Resources

All IDs are strings.

| Resource | ID field | Example |
|---|---|---|
| foods | `foodId` | `06.178` |
| food-groups | `foodGroupId` | `1`, `1.4.5` |
| nutrients | `nutrientId` | `Vann`, `Fe`, `Vit E` |
| sources | `sourceId` | `0`, `104b`, `MI0115` |
| langual | `langualCode` | `A0001` |
| rda | `id` | `rda-1179869501` |

## Commands

```
mvt foods list                              # all foods
mvt foods get <foodId>                      # full record
mvt foods search <query> [--limit N]        # edge-ngram + diacritic-folded
mvt foods rda <foodId> [--profile <id>]     # per-nutrient RDA coverage

mvt food-groups list | get <id>
mvt nutrients   list | get <id>
mvt sources     list | get <id>
mvt langual     list | get <id>             # no --locale effect
mvt rda         list | get <id>

mvt describe [--check-upstream]             # schema + data version; optional HEAD probe
mvt update [--check-only]                   # self-update from GitHub releases
```

## Patterns agents should default to

```bash
# List a slim projection (avoid 13 MB of output)
mvt foods list --fields foodId,foodName,foodGroupId --json

# Search returns compact records by default; cap with --limit
mvt foods search "egg" --limit 20 --fields foodId,foodName,foodGroupId --json

# Drill into specific nutrient quantities of a single food
mvt foods get 06.178 --fields "foodId,foodName,constituents" --json
# then jq the constituents list; each entry has {nutrientId, quantity, unit, sourceId}

# RDA coverage: default profile is "Generell 18-70 år"
mvt foods rda 06.178 --json
```

## Known limitations

- Data freshness tied to release cadence. Run `mvt describe --check-upstream` to see if upstream has updated. Run `mvt update` to pull a fresh binary with newer embedded data.
- `--locale en` is strict: a record absent from the English export errors out instead of falling back to `nb`. If you want either-or, try both locales.
- Not all nutrients map to an energy factor. `foods rda` reports those in `missingNutrients`.
