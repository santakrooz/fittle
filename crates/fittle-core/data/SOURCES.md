# Data sources

Synced from AstroSideKick at commit 1297b75.

| File | Source | Licence |
| --- | --- | --- |
| `scope-profiles.json` | AstroSideKick `packages/scope-profiles/src/registry.json` (smart-scope match rules + hardware) | Project's own |
| `targets.json`, `targets-extra.json`, `targets-names.json` | AstroSideKick subset of **OpenNGC** by Mattia Verga (https://github.com/mattiaverga/OpenNGC); names partly from Wikidata (CC0) | **CC BY-SA 4.0** (share-alike applies to these data files only) |
| `apps.json` | Fittle's own: capture-app / stacker fingerprints and vendor quirks | Project's own |

Do not edit the synced files here. Change them in AstroSideKick and run
`scripts/sync-astrosidekick-data.sh`. Corrections found while building Fittle
(e.g. from Dwarf documentation) should go upstream first.
