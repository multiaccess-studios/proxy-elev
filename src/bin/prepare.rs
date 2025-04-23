use std::{
    collections::{BTreeSet, HashMap, HashSet},
    io::Write,
    path::PathBuf,
};

use anyhow::Context;
use clap::Parser;
use proxy_elev::{
    AlternateFaceMetadata, CardFacePrintingId, CardId, CardMetadata, InsertId, InsertMetadata,
    Library, MultiLibrary, PrintingMetadata, Title,
};
use ron::ser::PrettyConfig;

/// Prepare files for NRO services.
#[derive(Parser, Debug)]
struct Opt {
    /// Path to the netrunner-cards-json directory
    netrunner_cards_json: PathBuf,
    /// Manifest of the printings
    manifest: PathBuf,
    /// Location to output the built artifact to
    output: PathBuf,
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
    };

    let manifest = std::fs::read_to_string(&opt.manifest)?;
    let manifest: toml::Table = toml::from_str(&manifest)?;

    let manifest_collections = manifest["collection"]
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

    std::fs::create_dir_all(opt.output.parent().unwrap())?;

    let mut write = std::fs::File::options()
        .write(true)
        .create(true)
        .truncate(true)
        .open(opt.output)?;

    let buf = ron::ser::to_string_pretty(&multi_library, PrettyConfig::default())?;
    write.write_all(buf.as_bytes())?;

    Ok(())
}
