# Frozen palette inputs

`theme-palettes.json` contains every `colors.toml` palette from the signed
`omarchy-4.5.0rc3-1-x86_64.pkg.tar.zst` package. Its source URL and package SHA256
are recorded in the JSON. The upstream MIT license is retained beside it.

The pure JavaScript suite checks the complete 23-name inventory, every palette's
presence and parseability, and all existing contrast rules. It reads no installed
Omarchy palettes, so a builder package update cannot change those inputs.
`tests/themes.sh` remains the separate native check against installed palettes.
