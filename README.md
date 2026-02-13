# NRO Proxy Service

## Regenerating Manifest

If you wish to regenerate the manifest, you will need the
[`netrunner-cards-json`](https://github.com/NetrunnerDB/netrunner-cards-json/) submodule.

```bash
cargo run --bin prepare -- .\netrunner-cards-json\ .\printing-manifest.toml .\src\manifest.ron
```

To inject preview cards not yet in netrunner-cards-json, add them directly to the manifest file.
For local-only additions, create a `printing-manifest.local.toml` next to the main manifest. It is
auto-detected (or pass `--local-manifest path\to\file.toml`) and can contain only `[[card]]` and
`[[nrdb_remap]]` sections, plus optional `[[local_image]]` overrides for card images. When present,
`prepare` emits a separate runtime overlay `local-assets/manifest.local.ron` (and does not merge
local entries into `src/manifest.ron`).

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

Local image override example (for local-only assets):

```toml
[[local_image]]
id = 36001
group = "english"
path = "E:\\VP Working\\webps\\36001.webp"
```

Notes:
- `path` is converted to a `file:///` URL; you can also provide `url` directly.
- If a printing has multiple faces or variants, specify `face = 1` (or 2, 3, ...) to select it.

You can also set a root for local assets once and just list IDs:

```toml
[local_image_root]
path = "E:\\VP Working\\webps"
url = "/local-assets"

[[local_image]]
id = 36001
group = "english"
```

When using `url = "/local-assets"`, the app will request `/local-assets/36001.webp`. Trunk is
configured to copy a `local-assets` directory from the repo root, and the app will also look for
`/local-assets/manifest.local.ron` at runtime. Create a directory junction or symlink named
`local-assets` that points to your real asset folder.

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
