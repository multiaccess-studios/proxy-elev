use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    io::Write,
    path::PathBuf,
};

use anyhow::Context;
use clap::Parser;
use proxy_elev::{
    AlternateFaceMetadata, CardFacePrintingId, CardId, CardMetadata, InsertId, InsertMetadata,
    Library, LocalImageOverride, MultiLibrary, PrintingMetadata, Title,
};
use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};

/// Prepare files for NRO services.
#[derive(Parser, Debug)]
struct Opt {
    /// Path to the netrunner-cards-json directory
    netrunner_cards_json: PathBuf,
    /// Manifest of the printings
    manifest: PathBuf,
    /// Optional local-only manifest extras (overrides `printing-manifest.local.toml` if provided)
    #[arg(long)]
    local_manifest: Option<PathBuf>,
    /// Optional local overlay output path (defaults to `local-assets/manifest.local.ron` when local manifest is present)
    #[arg(long)]
    local_output: Option<PathBuf>,
    /// Location to output the built artifact to
    output: PathBuf,
}

#[derive(Debug, Deserialize)]
struct ManifestExtras {
    #[serde(default)]
    card: Vec<ExtraCard>,
    #[serde(default)]
    nrdb_remap: Vec<NrdbRemap>,
    #[serde(default)]
    local_image: Vec<LocalImageOverrideInput>,
    #[serde(default)]
    local_image_root: Option<LocalImageRootInput>,
}

#[derive(Debug, Deserialize)]
struct ExtraCardsFile {
    #[serde(default)]
    card: Vec<ExtraCard>,
}

#[derive(Debug, Deserialize, Clone)]
struct ExtraCard {
    id: String,
    title: String,
    stripped_title: Option<String>,
    group: Option<String>,
    printing_name: Option<String>,
    printing_id: Option<u32>,
    #[serde(default)]
    printings: Vec<ExtraPrinting>,
    #[serde(default)]
    faces: Vec<String>,
    variants: Option<usize>,
}

#[derive(Debug, Deserialize, Clone)]
struct ExtraPrinting {
    id: u32,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NrdbRemap {
    from: u32,
    to: u32,
}

#[derive(Debug, Deserialize, Clone)]
struct LocalImageOverrideInput {
    id: u32,
    #[serde(default)]
    face: Option<usize>,
    #[serde(default)]
    group: Option<String>,
    url: Option<String>,
    path: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct LocalImageRootInput {
    url: Option<String>,
    path: Option<String>,
}

#[derive(Debug, Serialize)]
struct StableMultiLibrary {
    libraries: BTreeMap<String, StableLibrary>,
    collection_names: BTreeMap<String, String>,
    nrdb_remap: BTreeMap<u32, u32>,
    local_images: Vec<LocalImageOverride>,
}

#[derive(Debug, Serialize)]
struct StableLibrary {
    cards: BTreeMap<CardId, CardMetadata>,
    faces: BTreeMap<CardFacePrintingId, PrintingMetadata>,
    inserts: BTreeMap<InsertId, StableInsertMetadata>,
}

#[derive(Debug, Serialize)]
struct StableInsertMetadata {
    title: Title,
    id: InsertId,
    insert_groups: BTreeSet<String>,
}

impl From<&InsertMetadata> for StableInsertMetadata {
    fn from(value: &InsertMetadata) -> Self {
        Self {
            title: value.title.clone(),
            id: value.id.clone(),
            insert_groups: value.insert_groups.iter().cloned().collect(),
        }
    }
}

impl From<&Library> for StableLibrary {
    fn from(value: &Library) -> Self {
        Self {
            cards: value
                .cards
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            faces: value
                .faces
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            inserts: value
                .inserts
                .iter()
                .map(|(k, v)| (k.clone(), StableInsertMetadata::from(v)))
                .collect(),
        }
    }
}

impl From<&MultiLibrary> for StableMultiLibrary {
    fn from(value: &MultiLibrary) -> Self {
        let mut local_images = value.local_images.clone();
        local_images.sort_by(|a, b| {
            a.print_group
                .cmp(&b.print_group)
                .then_with(|| a.id.cmp(&b.id))
                .then_with(|| a.face_or_variant_specifier.cmp(&b.face_or_variant_specifier))
                .then_with(|| a.url.cmp(&b.url))
        });

        Self {
            libraries: value
                .libraries
                .iter()
                .map(|(k, v)| (k.clone(), StableLibrary::from(v)))
                .collect(),
            collection_names: value
                .collection_names
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            nrdb_remap: value
                .nrdb_remap
                .iter()
                .map(|(k, v)| (*k, *v))
                .collect(),
            local_images,
        }
    }
}

fn strip_non_ascii(title: &str) -> String {
    let stripped: String = title.chars().filter(|c| c.is_ascii()).collect();
    if stripped.is_empty() {
        title.to_string()
    } else {
        stripped
    }
}

fn insert_printing_face(
    library: &mut Library,
    card_meta: &mut CardMetadata,
    card_id: &CardId,
    print_group: &str,
    printing_id: u32,
    face_or_variant_specifier: Option<usize>,
    printing_name: &str,
) {
    let face = CardFacePrintingId {
        id: printing_id,
        face_or_variant_specifier,
        print_group: print_group.to_string(),
    };
    card_meta.printings.insert(face.clone());
    library.faces.insert(
        face.clone(),
        PrintingMetadata {
            id: face,
            card_id: card_id.clone(),
            printing_name: printing_name.to_string(),
        },
    );
}

fn build_extra_by_group(
    base_library: &MultiLibrary,
    extra_cards: ExtraCardsFile,
) -> anyhow::Result<HashMap<String, Library>> {
    let mut extra_by_group: HashMap<String, Library> = HashMap::new();

    for card in extra_cards.card {
        let group = card.group.unwrap_or_else(|| "english".to_string());
        if !base_library.libraries.contains_key(&group) {
            anyhow::bail!("Extra card group `{group}` does not exist in manifest");
        }

        if !card.faces.is_empty() && card.variants.is_some() {
            anyhow::bail!(
                "Extra card `{}` cannot define both `faces` and `variants`",
                card.id
            );
        }
        if let Some(variants) = card.variants {
            if variants < 2 {
                anyhow::bail!(
                    "Extra card `{}` has `variants` < 2; omit it for single-face cards",
                    card.id
                );
            }
        }

        if card.printing_id.is_some() && !card.printings.is_empty() {
            anyhow::bail!(
                "Extra card `{}` cannot define both `printing_id` and `printings`",
                card.id
            );
        }

        let mut printings = card.printings.clone();
        if printings.is_empty() {
            let printing_id = match card.printing_id {
                Some(printing_id) => printing_id,
                None => {
                    anyhow::bail!(
                        "Extra card `{}` missing `printing_id` or `printings`",
                        card.id
                    );
                }
            };
            printings.push(ExtraPrinting {
                id: printing_id,
                name: card.printing_name.clone(),
            });
        }

        let library = extra_by_group.entry(group.clone()).or_insert_with(|| Library {
            cards: HashMap::new(),
            faces: HashMap::new(),
            inserts: HashMap::new(),
        });

        let stripped_title = card
            .stripped_title
            .clone()
            .unwrap_or_else(|| strip_non_ascii(&card.title));
        let title = Title {
            title: card.title.clone(),
            stripped_title,
        };

        let alternate_faces: Vec<Title> = card
            .faces
            .iter()
            .map(|face| Title {
                title: face.clone(),
                stripped_title: strip_non_ascii(face),
            })
            .collect();
        let alternate_face_data = if !alternate_faces.is_empty() {
            AlternateFaceMetadata::Multiple(alternate_faces)
        } else if let Some(variants) = card.variants {
            AlternateFaceMetadata::Variants(variants)
        } else {
            AlternateFaceMetadata::Single
        };

        let card_id = CardId(card.id.clone());
        let mut card_meta = CardMetadata {
            title,
            alternate_face_data,
            id: card_id.clone(),
            printings: BTreeSet::new(),
        };

        if let Some(existing) = base_library.libraries[&group].cards.get(&card_id) {
            if existing.title != card_meta.title
                || existing.alternate_face_data != card_meta.alternate_face_data
            {
                anyhow::bail!(
                    "Extra card `{}` conflicts with existing card metadata",
                    card.id
                );
            }
        }

        for printing in printings {
            let printing_name = printing
                .name
                .clone()
                .or_else(|| card.printing_name.clone())
                .unwrap_or_else(|| "Custom".to_string());
            if !card.faces.is_empty() {
                insert_printing_face(
                    library,
                    &mut card_meta,
                    &card_id,
                    &group,
                    printing.id,
                    Some(1),
                    &printing_name,
                );
                for (i, _) in card.faces.iter().enumerate() {
                    insert_printing_face(
                        library,
                        &mut card_meta,
                        &card_id,
                        &group,
                        printing.id,
                        Some(i + 2),
                        &printing_name,
                    );
                }
            } else if let Some(variants) = card.variants {
                for variant in 1..=variants {
                    insert_printing_face(
                        library,
                        &mut card_meta,
                        &card_id,
                        &group,
                        printing.id,
                        Some(variant),
                        &printing_name,
                    );
                }
            } else {
                insert_printing_face(
                    library,
                    &mut card_meta,
                    &card_id,
                    &group,
                    printing.id,
                    None,
                    &printing_name,
                );
            }
        }

        match library.cards.entry(card_id.clone()) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                let existing = entry.get_mut();
                if existing.title != card_meta.title
                    || existing.alternate_face_data != card_meta.alternate_face_data
                {
                    anyhow::bail!(
                        "Extra card `{}` redefined with different title or face metadata",
                        card.id
                    );
                }
                existing.printings.extend(card_meta.printings.into_iter());
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(card_meta);
            }
        }
    }

    Ok(extra_by_group)
}

fn merge_extra_cards(
    multi_library: &mut MultiLibrary,
    extra_cards: ExtraCardsFile,
) -> anyhow::Result<()> {
    let extra_by_group = build_extra_by_group(multi_library, extra_cards)?;

    for (group, extra_library) in extra_by_group {
        if let Some(library) = multi_library.libraries.get_mut(&group) {
            library.merge(&extra_library);
        }
    }

    Ok(())
}

fn build_local_image_overrides(
    base_library: &MultiLibrary,
    extras: &ManifestExtras,
) -> anyhow::Result<Vec<LocalImageOverride>> {
    if extras.local_image.is_empty() {
        return Ok(Vec::new());
    }

    let root_url = extras
        .local_image_root
        .as_ref()
        .and_then(|root| root.url.as_ref())
        .cloned();
    let root_path = extras
        .local_image_root
        .as_ref()
        .and_then(|root| root.path.as_ref())
        .cloned();

    let mut overrides = Vec::with_capacity(extras.local_image.len());
    for override_ in &extras.local_image {
        if override_.url.is_none()
            && override_.path.is_none()
            && root_url.is_none()
            && root_path.is_none()
        {
            anyhow::bail!(
                "Local image override {} missing `url` or `path` and no `local_image_root`",
                override_.id
            );
        }
        if override_.url.is_some() && override_.path.is_some() {
            anyhow::bail!(
                "Local image override {} cannot define both `url` and `path`",
                override_.id
            );
        }

        let group = override_
            .group
            .clone()
            .unwrap_or_else(|| "english".to_string());
        let face = override_.face;

        let matches: Vec<CardFacePrintingId> = base_library
            .libraries
            .get(&group)
            .context(format!("Local image override group `{group}` missing"))?
            .faces
            .keys()
            .filter(|printing| printing.id == override_.id)
            .cloned()
            .collect();

        if matches.is_empty() {
            anyhow::bail!(
                "Local image override {} does not match any printings in group `{}`",
                override_.id,
                group
            );
        }

        let face_specifier = if let Some(face) = face {
            if !matches.iter().any(|printing| printing.face_or_variant_specifier == Some(face)) {
                anyhow::bail!(
                    "Local image override {} face {} does not exist in group `{}`",
                    override_.id,
                    face,
                    group
                );
            }
            Some(face)
        } else if matches.len() == 1 {
            matches[0].face_or_variant_specifier
        } else {
            anyhow::bail!(
                "Local image override {} matches multiple faces; specify `face`",
                override_.id
            );
        };

        let file_name = match face_specifier {
            Some(face) => format!("{}.{}.webp", override_.id, face),
            None => format!("{}.webp", override_.id),
        };

        let url = if let Some(url) = override_.url.clone() {
            url
        } else if let Some(path) = override_.path.clone() {
            let path = path.replace('\\', "/");
            if path.starts_with("file://") {
                path
            } else {
                format!("file:///{path}")
            }
        } else if let Some(root_url) = root_url.as_ref() {
            let base = root_url.trim_end_matches('/');
            format!("{base}/{file_name}")
        } else {
            let root = root_path
                .as_ref()
                .expect("root_path checked above")
                .replace('\\', "/")
                .trim_end_matches('/')
                .to_string();
            let path = format!("{root}/{file_name}");
            if path.starts_with("file://") {
                path
            } else {
                format!("file:///{path}")
            }
        };

        overrides.push(LocalImageOverride {
            id: override_.id,
            face_or_variant_specifier: face_specifier,
            print_group: group,
            url,
        });
    }

    Ok(overrides)
}

#[allow(clippy::too_many_lines)]
fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    if !std::fs::exists(&opt.netrunner_cards_json)? {
        anyhow::bail!("Either netrunner-cards-json directory does not exist.");
    }

    let mut multi_library = MultiLibrary {
        libraries: HashMap::new(),
        collection_names: HashMap::new(),
        nrdb_remap: HashMap::new(),
        local_images: Vec::new(),
    };

    let manifest = std::fs::read_to_string(&opt.manifest)?;
    let manifest_table: toml::Table = toml::from_str(&manifest)?;
    let extras: ManifestExtras = toml::from_str(&manifest)?;

    let local_manifest_path = opt.local_manifest.or_else(|| {
        let mut local = opt.manifest.clone();
        if let Some(stem) = local.file_stem().and_then(|s| s.to_str()) {
            local.set_file_name(format!("{stem}.local.toml"));
            if std::fs::exists(&local).ok()? {
                return Some(local);
            }
        }
        None
    });

    let local_extras = if let Some(path) = local_manifest_path.as_ref() {
        let local_manifest = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read local manifest {}", path.display()))?;
        Some(toml::from_str::<ManifestExtras>(&local_manifest)?)
    } else {
        None
    };

    let manifest_collections = manifest_table["collection"]
        .as_array()
        .context("`collection` not array")?;

    for manifest_collection in manifest_collections {
        let collection_name = manifest_collection["name"]
            .as_str()
            .context("`name` not a string")?;

        let manifest_printings = manifest_collection["printing"]
            .as_array()
            .context("`printing` not array")?;

        let manifest_inserts = manifest_collection["insert"]
            .as_array()
            .context("`insert` not array")?;

        let manifest_group = manifest_collection["group"]
            .as_str()
            .context("`group` not array")?;

        multi_library
            .collection_names
            .insert(manifest_group.into(), collection_name.into());

        let mut library = Library {
            cards: HashMap::new(),
            faces: HashMap::new(),
            inserts: HashMap::new(),
        };

        for insert in manifest_inserts {
            let insert_id = insert["id"].as_str().context("`id` not a string")?;
            let insert_title = insert["title"].as_str().context("`title` not a string")?;
            let insert_stripped_title = insert
                .get("stripped_title")
                .map(|title| title.as_str().context("`stripped_title` not a string"))
                .unwrap_or_else(|| Ok(insert_title))?;
            let insert_groups = insert
                .get("insert_groups")
                .map(|g| g.as_array().context("`insert_groups` not array"))
                .transpose()?
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|group| {
                    group
                        .as_str()
                        .context("`group` not a string")
                        .map(|s| s.to_string())
                })
                .collect::<Result<HashSet<_>, _>>()?;
            let insert_id = InsertId {
                name: insert_id.to_string(),
                print_group: manifest_group.to_string(),
            };
            let insert_meta = InsertMetadata {
                title: Title {
                    title: insert_title.into(),
                    stripped_title: insert_stripped_title.into(),
                },
                id: insert_id.clone(),
                insert_groups,
            };
            library
                .inserts
                .insert(insert_id.clone(), insert_meta.clone());
        }

        for manifest_printing in manifest_printings {
            let manifest_set = manifest_printing["spec"]
                .as_str()
                .context("`spec` not a string")?;
            let manifest_name = manifest_printing["name"]
                .as_str()
                .context("`name` not a string")?;

            let v2_printings: serde_json::Value = serde_json::from_reader(std::fs::File::open(
                opt.netrunner_cards_json
                    .join("v2")
                    .join("printings")
                    .join(format!("{manifest_set}.json")),
            )?)?;
            for printing in v2_printings
                .as_array()
                .context(format!("`{manifest_set}.json` not array"))?
            {
                let card_id = printing
                    .get("card_id")
                    .context("`card_id` not found")?
                    .as_str()
                    .context("`card_id` not a string")?;

                let card_id = CardId(card_id.to_string());

                let id = printing
                    .get("id")
                    .context("`id` not found")?
                    .as_str()
                    .context("`id` not a string")?
                    .parse()?;

                let printing_faces = printing
                    .get("faces")
                    .map(|faces| faces.as_array().context("`faces` not array"))
                    .transpose()?;

                let library_entry = library.cards.entry(card_id.clone());
                match library_entry {
                    std::collections::hash_map::Entry::Occupied(mut occupied_entry) => {
                        let card_metadata = occupied_entry.get_mut();
                        let printing_faces = printing
                            .get("faces")
                            .map(|faces| faces.as_array().context("`faces` not array"))
                            .transpose()?;
                        if let Some(faces) = printing_faces {
                            let face = CardFacePrintingId {
                                id,
                                face_or_variant_specifier: Some(1),
                                print_group: manifest_group.into(),
                            };
                            card_metadata.printings.insert(face.clone());
                            library.faces.insert(
                                face.clone(),
                                PrintingMetadata {
                                    id: face,
                                    card_id: card_id.clone(),
                                    printing_name: manifest_name.into(),
                                },
                            );
                            for (face, _) in faces.iter().enumerate() {
                                let face = CardFacePrintingId {
                                    id,
                                    face_or_variant_specifier: Some(face + 2),
                                    print_group: manifest_group.into(),
                                };
                                card_metadata.printings.insert(face.clone());
                                library.faces.insert(
                                    face.clone(),
                                    PrintingMetadata {
                                        id: face,
                                        card_id: card_id.clone(),
                                        printing_name: manifest_name.into(),
                                    },
                                );
                            }
                        } else {
                            let face = CardFacePrintingId {
                                id,
                                face_or_variant_specifier: None,
                                print_group: manifest_group.into(),
                            };
                            card_metadata.printings.insert(face.clone());
                            library.faces.insert(
                                face.clone(),
                                PrintingMetadata {
                                    id: face,
                                    card_id,
                                    printing_name: manifest_name.into(),
                                },
                            );
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(library_entry) => {
                        let card_data: serde_json::Value =
                            serde_json::from_reader(std::fs::File::open(
                                opt.netrunner_cards_json
                                    .join("v2")
                                    .join("cards")
                                    .join(format!("{card_id}.json", card_id = card_id.0)),
                            )?)?;

                        let card_title = card_data
                            .get("title")
                            .context("`title` not found")?
                            .as_str()
                            .context("`title` not a string")?;

                        let card_stripped_title = card_data
                            .get("stripped_title")
                            .context("`stripped_title` not found")?
                            .as_str()
                            .context("`stripped_title` not a string")?;

                        let title = Title {
                            title: card_title.into(),
                            stripped_title: card_stripped_title.into(),
                        };

                        let card_faces = card_data
                            .get("faces")
                            .map(|faces| faces.as_array().context("`faces` not array"))
                            .transpose()?;

                        let card = match (printing_faces, card_faces) {
                            // Flip card, such as hoshiko
                            (_, Some(card_faces)) => {
                                let mut alternate_faces = Vec::new();
                                let mut printings = BTreeSet::new();
                                let face = CardFacePrintingId {
                                    id,
                                    face_or_variant_specifier: Some(1),
                                    print_group: manifest_group.into(),
                                };
                                printings.insert(face.clone());
                                library.faces.insert(
                                    face.clone(),
                                    PrintingMetadata {
                                        id: face,
                                        card_id: card_id.clone(),
                                        printing_name: manifest_name.into(),
                                    },
                                );
                                for (i, face) in card_faces.iter().enumerate() {
                                    let title = face
                                        .get("title")
                                        .context("`title` not found")?
                                        .as_str()
                                        .context("`title` not a string")?;
                                    let stripped_title = face
                                        .get("stripped_title")
                                        .context("`stripped_title` not found")?
                                        .as_str()
                                        .context("`stripped_title` not a string")?;
                                    alternate_faces.push(Title {
                                        title: title.into(),
                                        stripped_title: stripped_title.into(),
                                    });
                                    let face = CardFacePrintingId {
                                        id,
                                        face_or_variant_specifier: Some(i + 2),
                                        print_group: manifest_group.into(),
                                    };
                                    printings.insert(face.clone());
                                    library.faces.insert(
                                        face.clone(),
                                        PrintingMetadata {
                                            id: face,
                                            card_id: card_id.clone(),
                                            printing_name: manifest_name.into(),
                                        },
                                    );
                                }
                                CardMetadata {
                                    title,
                                    alternate_face_data: AlternateFaceMetadata::Multiple(
                                        alternate_faces,
                                    ),
                                    id: card_id.clone(),
                                    printings,
                                }
                            }
                            // Single card
                            (None, None) => {
                                let face = CardFacePrintingId {
                                    id,
                                    face_or_variant_specifier: None,
                                    print_group: manifest_group.into(),
                                };
                                library.faces.insert(
                                    face.clone(),
                                    PrintingMetadata {
                                        id: face.clone(),
                                        card_id: card_id.clone(),
                                        printing_name: manifest_name.into(),
                                    },
                                );
                                CardMetadata {
                                    title,
                                    alternate_face_data: AlternateFaceMetadata::Single,
                                    id: card_id.clone(),
                                    printings: BTreeSet::from([face.clone()]),
                                }
                            }
                            // Variant card, such as matryoshka
                            (Some(variants), None) => {
                                let mut printings = BTreeSet::new();
                                let face = CardFacePrintingId {
                                    id,
                                    face_or_variant_specifier: Some(1),
                                    print_group: manifest_group.into(),
                                };
                                printings.insert(face.clone());
                                library.faces.insert(
                                    face.clone(),
                                    PrintingMetadata {
                                        id: face,
                                        card_id: card_id.clone(),
                                        printing_name: manifest_name.into(),
                                    },
                                );

                                for (variant, _) in variants.iter().enumerate() {
                                    let face = CardFacePrintingId {
                                        id,
                                        face_or_variant_specifier: Some(variant + 2),
                                        print_group: manifest_group.into(),
                                    };
                                    printings.insert(face.clone());
                                    library.faces.insert(
                                        face.clone(),
                                        PrintingMetadata {
                                            id: face,
                                            card_id: card_id.clone(),
                                            printing_name: manifest_name.into(),
                                        },
                                    );
                                }
                                CardMetadata {
                                    title,
                                    alternate_face_data: AlternateFaceMetadata::Variants(
                                        variants.len() + 1,
                                    ),
                                    id: card_id.clone(),
                                    printings,
                                }
                            }
                        };
                        library_entry.insert_entry(card);
                    }
                };
            }
        }

        multi_library
            .libraries
            .insert(manifest_group.into(), library);
    }

    if !extras.card.is_empty() {
        merge_extra_cards(
            &mut multi_library,
            ExtraCardsFile {
                card: extras.card,
            },
        )?;
    }

    if !extras.nrdb_remap.is_empty() {
        let mut remap = HashMap::new();
        for mapping in extras.nrdb_remap {
            remap.insert(mapping.from, mapping.to);
        }
        for (&from, &to) in &remap {
            let mut found = false;
            for library in multi_library.libraries.values() {
                if library.faces.keys().any(|face| face.id == to) {
                    found = true;
                    break;
                }
            }
            if !found {
                anyhow::bail!(
                    "NRDB remap target {} (from {}) does not exist in any library",
                    to,
                    from
                );
            }
        }
        multi_library.nrdb_remap = remap;
    }

    let local_overlay = if let Some(local_extras) = local_extras {
        let has_local = !local_extras.card.is_empty()
            || !local_extras.nrdb_remap.is_empty()
            || !local_extras.local_image.is_empty();
        if !has_local {
            None
        } else {
            let extra_by_group = build_extra_by_group(
                &multi_library,
                ExtraCardsFile {
                    card: local_extras.card.clone(),
                },
            )?;

            let mut overlay = MultiLibrary {
                libraries: HashMap::new(),
                collection_names: HashMap::new(),
                nrdb_remap: HashMap::new(),
                local_images: Vec::new(),
            };

            for (group, library) in &extra_by_group {
                if let Some(name) = multi_library.collection_names.get(group) {
                    overlay.collection_names.insert(group.clone(), name.clone());
                }
                overlay.libraries.insert(group.clone(), library.clone());
            }

            let mut validation_library = multi_library.clone();
            for (group, library) in &overlay.libraries {
                match validation_library.libraries.get_mut(group) {
                    Some(existing) => existing.merge(library),
                    None => {
                        validation_library
                            .libraries
                            .insert(group.clone(), library.clone());
                    }
                }
            }

            if !local_extras.nrdb_remap.is_empty() {
                let mut remap = HashMap::new();
                for mapping in &local_extras.nrdb_remap {
                    remap.insert(mapping.from, mapping.to);
                }
                for (&from, &to) in &remap {
                    let mut found = false;
                    for library in validation_library.libraries.values() {
                        if library.faces.keys().any(|face| face.id == to) {
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        anyhow::bail!(
                            "NRDB remap target {} (from {}) does not exist in any library",
                            to,
                            from
                        );
                    }
                }
                overlay.nrdb_remap = remap;
            }

            overlay.local_images = build_local_image_overrides(&validation_library, &local_extras)?;

            Some(overlay)
        }
    } else {
        None
    };

    std::fs::create_dir_all(opt.output.parent().unwrap())?;

    let mut write = std::fs::File::options()
        .write(true)
        .create(true)
        .truncate(true)
        .open(opt.output)?;

    let stable_manifest = StableMultiLibrary::from(&multi_library);
    let buf = ron::ser::to_string_pretty(&stable_manifest, PrettyConfig::default())?;
    write.write_all(buf.as_bytes())?;

    if let Some(local_overlay) = local_overlay {
        let local_output = opt
            .local_output
            .clone()
            .unwrap_or_else(|| PathBuf::from("local-assets/manifest.local.ron"));
        if let Some(parent) = local_output.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut write = std::fs::File::options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(local_output)?;
        let stable_overlay = StableMultiLibrary::from(&local_overlay);
        let buf = ron::ser::to_string_pretty(&stable_overlay, PrettyConfig::default())?;
        write.write_all(buf.as_bytes())?;
    }

    Ok(())
}
