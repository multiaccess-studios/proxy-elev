# NRO Proxy Service

## Regenerating Manifest

If you wish to regenerate the manifest, you will need the
[`netrunner-cards-json`](https://github.com/NetrunnerDB/netrunner-cards-json/) submodule.

```bash
cargo run --bin prepare -- .\netrunner-cards-json\ .\printing-manifest.toml .\src\manifest.ron
```

To inject preview cards not yet in netrunner-cards-json, add them directly to the manifest file.

Example additions to `printing-manifest.toml`:

```toml
[[card]]
id = "33001"
title = "Preview Card"
group = "english"
printing_id = 99001
printing_name = "Preview (English)"

[[card]]
id = "33002"
title = "Flip Example"
group = "english"
printing_id = 99002
faces = ["Flip Example (B)"]

[[card]]
id = "33003"
title = "Variant Example"
group = "english"
printing_id = 99003
variants = 3
```

Notes:
- `stripped_title` is optional; if omitted, it is derived by removing non-ASCII characters.
- Use `printings = [{ id = 99004, name = "Preview" }]` instead of `printing_id` when you need multiple printings or per-printing names.

NRDB remap example (used during NRDB import):

```toml
[[nrdb_remap]]
from = 32003
to = 33022
```

## Generating Arts

**Note:** To use this, you will need a source of artwork, if you are internal to NSG and have access
to the full bleed files, square cornered compressed 300dpi webp images as used on the current live
website can be generated through the following process:

Process Arts

```bash
magick mogrify -gravity Center -crop 1500x2100+0+0 +repage -format jpg -quality 100 -resize 750x1050! -path webps *.jpg
```

Replace `*.jpg` to do it for a single art.

```bash
ls -file *.jpg | % { cwebp -quiet -q 95 $_.fullname -o $_.FullName.Replace(".jpg", ".webp") }
```

Remove prefix from files

```bash
ls -file *.webp | % { Rename-Item $_.fullname $_.FullName.Replace("Card-Ashes-Uprising_", "") }
```

Fix numbers

```bash
cargo run --bin namerebase -- ./path/to/files webp 26065
```
