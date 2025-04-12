# NRO Proxy Service

## Regenerating Manifest

If you wish to regenerate the manifest, you will need the
[`netrunner-cards-json`](https://github.com/NetrunnerDB/netrunner-cards-json/) submodule.

```bash
cargo run --bin prepare -- .\netrunner-cards-json\ .\printing-manifest.toml .\src\manifest.ron
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
