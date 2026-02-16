use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

pub const CARD_IMAGE_URL_ROOT: &str = match option_env!("NRO_PROXY_CARD_IMAGE_URL_ROOT") {
    Some(env) => env,
    None => "https://nro-public.s3.nl-ams.scw.cloud/nro/card-printings/v2/webp",
};

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum CardImage {
    CardFacePrinting(CardFacePrintingId),
    Insert(InsertId),
}

impl CardImage {
    #[must_use]
    pub fn image_url(&self) -> String {
        match self {
            CardImage::CardFacePrinting(image) => image.image_url(),
            CardImage::Insert(insert) => insert.image_url(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct CardFacePrintingId {
    pub id: u32,
    pub face_or_variant_specifier: Option<usize>,
    pub print_group: String,
}
impl CardFacePrintingId {
    #[must_use]
    pub fn image_url(&self) -> String {
        match self.face_or_variant_specifier {
            Some(face) => format!(
                "{CARD_IMAGE_URL_ROOT}/{group}/card/{id:5>0}.{face}.webp",
                group = self.print_group,
                id = self.id
            ),
            None => format!(
                "{CARD_IMAGE_URL_ROOT}/{group}/card/{id:5>0}.webp",
                group = self.print_group,
                id = self.id
            ),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct InsertId {
    pub name: String,
    pub print_group: String,
}
impl InsertId {
    #[must_use]
    pub fn image_url(&self) -> String {
        format!(
            "{CARD_IMAGE_URL_ROOT}/{group}/insert/{name}.webp",
            group = self.print_group,
            name = self.name
        )
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct CardId(pub String);

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct MultiLibrary {
    pub libraries: HashMap<String, Library>,
    pub collection_names: HashMap<String, String>,
    #[serde(default)]
    pub nrdb_remap: HashMap<u32, u32>,
    #[serde(default)]
    pub local_images: Vec<LocalImageOverride>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct LocalImageOverride {
    pub id: u32,
    pub face_or_variant_specifier: Option<usize>,
    pub print_group: String,
    pub url: String,
}

impl MultiLibrary {
    #[must_use]
    pub fn local_image_url(&self, printing: &CardFacePrintingId) -> Option<&str> {
        self.local_images
            .iter()
            .find(|override_| {
                override_.id == printing.id
                    && override_.print_group == printing.print_group
                    && override_.face_or_variant_specifier == printing.face_or_variant_specifier
            })
            .map(|override_| override_.url.as_str())
    }

    pub fn merge_overlay(&mut self, overlay: MultiLibrary) {
        for (group, library) in overlay.libraries {
            match self.libraries.get_mut(&group) {
                Some(existing) => existing.merge(&library),
                None => {
                    self.libraries.insert(group, library);
                }
            }
        }
        for (group, name) in overlay.collection_names {
            self.collection_names.insert(group, name);
        }
        for (from, to) in overlay.nrdb_remap {
            self.nrdb_remap.insert(from, to);
        }
        self.local_images.extend(overlay.local_images);
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub cards: HashMap<CardId, CardMetadata>,
    pub faces: HashMap<CardFacePrintingId, PrintingMetadata>,
    pub inserts: HashMap<InsertId, InsertMetadata>,
}
impl Library {
    #[must_use]
    pub fn try_get_card(&self, id: &CardId) -> Option<&CardMetadata> {
        self.cards.get(id)
    }
    #[must_use]
    pub fn get_card(&self, id: &CardId) -> &CardMetadata {
        &self.cards[id]
    }
    #[must_use]
    pub fn get_face_card(&self, id: &CardFacePrintingId) -> &CardMetadata {
        let card = &self.faces[id].card_id;
        &self.cards[card]
    }
    #[must_use]
    pub fn try_get_face_card(&self, id: &CardFacePrintingId) -> Option<&CardMetadata> {
        let card_id = self.faces.get(id).map(|printing| &printing.card_id)?;
        self.cards.get(card_id)
    }
    #[must_use]
    pub fn get_insert(&self, id: &InsertId) -> &InsertMetadata {
        &self.inserts[id]
    }
    pub fn merge(&mut self, other: &Library) {
        for (card, meta) in &other.cards {
            self.cards
                .entry(card.clone())
                .or_insert_with(|| CardMetadata {
                    title: meta.title.clone(),
                    alternate_face_data: meta.alternate_face_data.clone(),
                    id: meta.id.clone(),
                    printings: BTreeSet::new(),
                })
                .printings
                .extend(meta.printings.iter().cloned());
        }
        for (face, card) in &other.faces {
            if !self.faces.contains_key(face) {
                self.faces.insert(face.clone(), card.clone());
            }
        }
        for (insert, meta) in &other.inserts {
            if !self.inserts.contains_key(insert) {
                self.inserts.insert(insert.clone(), meta.clone());
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct PrintingMetadata {
    pub id: CardFacePrintingId,
    pub card_id: CardId,
    pub printing_name: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct InsertMetadata {
    pub title: Title,
    pub id: InsertId,
    pub insert_groups: HashSet<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct CardMetadata {
    /// The title of the card, as supplied by its card data in NRDB
    pub title: Title,
    /// The other faces the card has, as supplied by its card data in NRDB or
    /// its printing data.
    pub alternate_face_data: AlternateFaceMetadata,
    /// The ID of the card in NRDB
    pub id: CardId,
    /// The ID of the cards printings in NRDB
    pub printings: BTreeSet<CardFacePrintingId>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum AlternateFaceMetadata {
    /// The card has only a single face with no variants.
    Single,
    /// The card has multiple faces, each with their own titles.
    Multiple(Vec<Title>),
    /// The card has only one face, but that face has multiple variant forms
    /// such as matryoshka.
    Variants(usize),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Title {
    /// The title of the card, possibly containing non-ASCII characters.
    pub title: String,
    /// The title of the card, reduced to ASCII characters.
    pub stripped_title: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum FilledCardSlot {
    Card { printing: CardFacePrintingId },
    Insert { insert: InsertId },
}
impl FilledCardSlot {
    #[must_use]
    pub fn is_local_override(&self) -> bool {
        match self {
            FilledCardSlot::Card { printing } => ACTIVE_LIBRARY
                .read()
                .expect("library lock")
                .local_image_url(printing)
                .is_some(),
            FilledCardSlot::Insert { .. } => false,
        }
    }

    #[must_use]
    pub fn image_url(&self) -> String {
        match self {
            FilledCardSlot::Card { printing } => ACTIVE_LIBRARY
                .read()
                .expect("library lock")
                .local_image_url(printing)
                .map(str::to_string)
                .unwrap_or_else(|| printing.image_url()),
            FilledCardSlot::Insert { insert } => insert.image_url(),
        }
    }
    #[must_use]
    pub fn name(&self) -> String {
        let library = ACTIVE_LIBRARY.read().expect("library lock");
        match self {
            FilledCardSlot::Card { printing } => {
                let Some(card) = library
                    .libraries
                    .get(&printing.print_group)
                    .and_then(|group| group.try_get_face_card(printing))
                else {
                    return format!("Missing card {} ({})", printing.id, printing.print_group);
                };

                match printing.face_or_variant_specifier {
                    None | Some(1) => card.title.title.clone(),
                    Some(n) => match &card.alternate_face_data {
                        AlternateFaceMetadata::Single | AlternateFaceMetadata::Variants(_) => {
                            card.title.title.clone()
                        }
                        AlternateFaceMetadata::Multiple(titles) => titles
                            .get(n.saturating_sub(2))
                            .map_or_else(|| card.title.title.clone(), |title| title.title.clone()),
                    },
                }
            }
            FilledCardSlot::Insert { insert } => library
                .libraries
                .get(&insert.print_group)
                .and_then(|group| group.inserts.get(insert))
                .map_or_else(
                    || format!("Missing insert {} ({})", insert.name, insert.print_group),
                    |insert| insert.title.title.clone(),
                ),
        }
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PrintFile {
    slots: Vec<FilledCardSlot>,
    auto_faces: HashMap<(CardId, usize), usize>,
}
impl PrintFile {
    pub fn clear(&mut self) {
        self.slots.clear();
        self.auto_faces.clear();
    }
    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
    #[must_use]
    pub fn all(&self) -> &[FilledCardSlot] {
        &self.slots
    }
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&FilledCardSlot> {
        self.slots.get(index)
    }
    pub fn add_insert(&mut self, insert: InsertId) {
        self.slots.push(FilledCardSlot::Insert { insert });
    }
    pub fn remove_card(&mut self, index: usize) {
        if index < self.slots.len() {
            let slot = self.slots.remove(index);
            if let &FilledCardSlot::Card {
                printing:
                    ref face @ CardFacePrintingId {
                        face_or_variant_specifier: Some(variant),
                        ref print_group,
                        ..
                    },
                } = &slot
            {
                if let Some(CardMetadata {
                    alternate_face_data: AlternateFaceMetadata::Variants(_),
                    id,
                    ..
                }) = ACTIVE_LIBRARY
                    .read()
                    .expect("library lock")
                    .libraries
                    .get(print_group)
                    .and_then(|library| library.try_get_face_card(face))
                {
                    let auto_faces = self.auto_faces.entry((id.clone(), variant)).or_default();
                    *auto_faces = auto_faces.saturating_sub(1);
                }
            }
        }
    }
    pub fn update_card(&mut self, index: usize, card: CardFacePrintingId) {
        if let Some(slot) = self.slots.get_mut(index) {
            if let &FilledCardSlot::Card {
                printing:
                    ref face @ CardFacePrintingId {
                        face_or_variant_specifier: Some(variant),
                        ..
                    },
            } = &*slot
            {
                if let Some(CardMetadata {
                    alternate_face_data: AlternateFaceMetadata::Variants(_),
                    id,
                    ..
                }) = ACTIVE_LIBRARY
                    .read()
                    .expect("library lock")
                    .libraries
                    .get(&card.print_group)
                    .and_then(|library| library.try_get_face_card(face))
                {
                    let auto_faces = self.auto_faces.entry((id.clone(), variant)).or_default();
                    *auto_faces = auto_faces.saturating_sub(1);
                }
            }
            *slot = FilledCardSlot::Card { printing: card };
        }
    }
    #[allow(clippy::missing_panics_doc)]
    pub fn add_cards(&mut self, meta: &CardMetadata) {
        match &meta.alternate_face_data {
            AlternateFaceMetadata::Single => self.slots.push(FilledCardSlot::Card {
                printing: meta.printings.last().cloned().expect("No printings"),
            }),
            AlternateFaceMetadata::Multiple(titles) => {
                let face = meta
                    .printings
                    .iter()
                    .rev()
                    .find(|f| f.face_or_variant_specifier == Some(1))
                    .unwrap();
                self.slots.push(FilledCardSlot::Card {
                    printing: face.clone(),
                });

                for (face, _) in titles.iter().enumerate() {
                    let face = meta
                        .printings
                        .iter()
                        .rev()
                        .find(|f| f.face_or_variant_specifier == Some(face + 2))
                        .unwrap();
                    self.slots.push(FilledCardSlot::Card {
                        printing: face.clone(),
                    });
                }
            }
            &AlternateFaceMetadata::Variants(variants) => {
                let mut variants_observed = usize::MAX;
                let mut next_variant = 1;
                for variant in 1..=variants {
                    let observed = self
                        .auto_faces
                        .get(&(meta.id.clone(), variant))
                        .copied()
                        .unwrap_or_default();
                    if observed < variants_observed {
                        variants_observed = observed;
                        next_variant = variant;
                    }
                }
                *self
                    .auto_faces
                    .entry((meta.id.clone(), next_variant))
                    .or_default() += 1;
                let face = meta
                    .printings
                    .iter()
                    .rev()
                    .find(|f| f.face_or_variant_specifier == Some(next_variant))
                    .unwrap();
                self.slots.push(FilledCardSlot::Card {
                    printing: face.clone(),
                });
            }
        }
    }
}

pub const MANIFEST: &str = include_str!("manifest.ron");
pub static MULTI_LIBRARY: std::sync::LazyLock<MultiLibrary> = std::sync::LazyLock::new(manifest);
pub static ACTIVE_LIBRARY: std::sync::LazyLock<RwLock<MultiLibrary>> =
    std::sync::LazyLock::new(|| RwLock::new(manifest()));

#[allow(clippy::missing_panics_doc)]
#[must_use]
pub fn manifest() -> MultiLibrary {
    ron::de::from_str(MANIFEST).expect("Failed to parse manifest")
}

const IN_TO_MM: f32 = 25.4;
const PT_TO_IN: f32 = 1.0 / 72.0;
const PT_TO_MM: f32 = PT_TO_IN * IN_TO_MM;
const A4_WIDTH: f32 = 210.0;
const A4_HEIGHT: f32 = 297.0;
const US_LETTER_WIDTH: f32 = 8.5 * IN_TO_MM;
const US_LETTER_HEIGHT: f32 = 11.0 * IN_TO_MM;

#[derive(Debug, Copy, Default, Clone, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum PrintSize {
    #[default]
    A4,
    UsLetter,
}
impl PrintSize {
    const fn size(self) -> (f32, f32) {
        match self {
            PrintSize::A4 => (A4_WIDTH, A4_HEIGHT),
            PrintSize::UsLetter => (US_LETTER_WIDTH, US_LETTER_HEIGHT),
        }
    }
}
impl std::fmt::Display for PrintSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrintSize::A4 => "A4".fmt(f),
            PrintSize::UsLetter => "US Letter".fmt(f),
        }
    }
}

#[derive(Debug, Copy, Default, Clone, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum CutIndicator {
    #[default]
    Lines,
    Marks,
    None,
}
impl std::fmt::Display for CutIndicator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CutIndicator::Lines => "Lines".fmt(f),
            CutIndicator::Marks => "Marks".fmt(f),
            CutIndicator::None => "None".fmt(f),
        }
    }
}

#[derive(Debug, Copy, Default, Clone, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum BleedMode {
    #[default]
    None,
    Narrow,
    Medium,
    Wide,
}
impl BleedMode {
    #[must_use]
    pub const fn bleed(self) -> f32 {
        match self {
            BleedMode::None => 0.0,
            BleedMode::Narrow => 2.0,
            BleedMode::Medium => 4.0,
            BleedMode::Wide => 6.0,
        }
    }
}
impl std::fmt::Display for BleedMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BleedMode::None => "None".fmt(f),
            BleedMode::Narrow => "Narrow".fmt(f),
            BleedMode::Medium => "Medium".fmt(f),
            BleedMode::Wide => "Wide".fmt(f),
        }
    }
}

const TRUE_CARD_WIDTH: f32 = 2.5 * IN_TO_MM;
const TRUE_CARD_HEIGHT: f32 = 3.5 * IN_TO_MM;
const CARD_WIDTH: f32 = TRUE_CARD_WIDTH * 0.98;
const CARD_HEIGHT: f32 = TRUE_CARD_HEIGHT * 0.98;

#[derive(Debug, Copy, Default, Clone, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct PrintConfig {
    pub print_size: PrintSize,
    pub cut_indicator: CutIndicator,
    pub bleed_mode: BleedMode,
}
impl PrintConfig {
    #[must_use]
    pub const fn paper(&self) -> (f32, f32) {
        self.print_size.size()
    }

    #[must_use]
    const fn precalc(self) -> ((f32, f32), (f32, f32), f32) {
        let (paper_width, paper_height) = self.paper();
        let postscale_width = CARD_WIDTH - self.bleed_mode.bleed();
        let true_scale = postscale_width / TRUE_CARD_WIDTH;
        let scale = postscale_width / CARD_WIDTH;
        let postcale_height = CARD_HEIGHT * scale;
        let scale_horizontal_offset = (CARD_WIDTH - postscale_width) / 2.0;
        let scale_vertical_offset = (CARD_HEIGHT - postcale_height) / 2.0;
        let global_horizontal_offset = (paper_width - (CARD_WIDTH * 3.0)) / 2.0;
        let global_vertical_offset = (paper_height - (CARD_HEIGHT * 3.0)) / 2.0;
        (
            (scale_horizontal_offset, global_horizontal_offset),
            (scale_vertical_offset, global_vertical_offset),
            true_scale,
        )
    }

    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn slot(&self, n: usize) -> (f32, f32, f32) {
        let (
            (scale_horizontal_offset, global_horizontal_offset),
            (scale_vertical_offset, global_vertical_offset),
            scale,
        ) = self.precalc();

        let card_horizontal_offset = ((n % 3) as f32) * CARD_WIDTH;
        let card_vertical_offset = ((2 - (n / 3)) as f32) * CARD_HEIGHT;

        (
            card_horizontal_offset + global_horizontal_offset + scale_horizontal_offset,
            card_vertical_offset + global_vertical_offset + scale_vertical_offset,
            scale,
        )
    }

    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn marks(&self) -> Vec<(f32, f32, f32, f32)> {
        match self.cut_indicator {
            CutIndicator::Lines => {
                let ((_, global_horizontal_offset), (_, global_vertical_offset), _) =
                    self.precalc();
                vec![
                    (
                        global_horizontal_offset - (0.5 * PT_TO_MM),
                        global_horizontal_offset + (0.5 * PT_TO_MM),
                        global_vertical_offset - (0.25 * IN_TO_MM),
                        global_vertical_offset + (CARD_HEIGHT * 3.0) + (0.25 * IN_TO_MM),
                    ),
                    (
                        global_horizontal_offset - (0.5 * PT_TO_MM) + CARD_WIDTH,
                        global_horizontal_offset + (0.5 * PT_TO_MM) + CARD_WIDTH,
                        global_vertical_offset - (0.25 * IN_TO_MM),
                        global_vertical_offset + (CARD_HEIGHT * 3.0) + (0.25 * IN_TO_MM),
                    ),
                    (
                        global_horizontal_offset - (0.5 * PT_TO_MM) + (CARD_WIDTH * 2.0),
                        global_horizontal_offset + (0.5 * PT_TO_MM) + (CARD_WIDTH * 2.0),
                        global_vertical_offset - (0.25 * IN_TO_MM),
                        global_vertical_offset + (CARD_HEIGHT * 3.0) + (0.25 * IN_TO_MM),
                    ),
                    (
                        global_horizontal_offset - (0.5 * PT_TO_MM) + (CARD_WIDTH * 3.0),
                        global_horizontal_offset + (0.5 * PT_TO_MM) + (CARD_WIDTH * 3.0),
                        global_vertical_offset - (0.25 * IN_TO_MM),
                        global_vertical_offset + (CARD_HEIGHT * 3.0) + (0.25 * IN_TO_MM),
                    ),
                    (
                        global_horizontal_offset - (0.25 * IN_TO_MM),
                        global_horizontal_offset + (CARD_WIDTH * 3.0) + (0.25 * IN_TO_MM),
                        global_vertical_offset - (0.5 * PT_TO_MM),
                        global_vertical_offset + (0.5 * PT_TO_MM),
                    ),
                    (
                        global_horizontal_offset - (0.25 * IN_TO_MM),
                        global_horizontal_offset + (CARD_WIDTH * 3.0) + (0.25 * IN_TO_MM),
                        global_vertical_offset - (0.5 * PT_TO_MM) + CARD_HEIGHT,
                        global_vertical_offset + (0.5 * PT_TO_MM) + CARD_HEIGHT,
                    ),
                    (
                        global_horizontal_offset - (0.25 * IN_TO_MM),
                        global_horizontal_offset + (CARD_WIDTH * 3.0) + (0.25 * IN_TO_MM),
                        global_vertical_offset - (0.5 * PT_TO_MM) + (2.0 * CARD_HEIGHT),
                        global_vertical_offset + (0.5 * PT_TO_MM) + (2.0 * CARD_HEIGHT),
                    ),
                    (
                        global_horizontal_offset - (0.25 * IN_TO_MM),
                        global_horizontal_offset + (CARD_WIDTH * 3.0) + (0.25 * IN_TO_MM),
                        global_vertical_offset - (0.5 * PT_TO_MM) + (3.0 * CARD_HEIGHT),
                        global_vertical_offset + (0.5 * PT_TO_MM) + (3.0 * CARD_HEIGHT),
                    ),
                ]
            }
            CutIndicator::Marks => {
                let mut marks = Vec::with_capacity(32);
                let ((_, global_horizontal_offset), (_, global_vertical_offset), _) =
                    self.precalc();
                for x in 0..=3 {
                    for y in 0..=3 {
                        marks.push((
                            global_horizontal_offset + (x as f32 * CARD_WIDTH) - (0.125 * IN_TO_MM),
                            global_horizontal_offset + (x as f32 * CARD_WIDTH) + (0.125 * IN_TO_MM),
                            global_vertical_offset + (y as f32 * CARD_HEIGHT) - (0.5 * PT_TO_MM),
                            global_vertical_offset + (y as f32 * CARD_HEIGHT) + (0.5 * PT_TO_MM),
                        ));
                        marks.push((
                            global_horizontal_offset + (x as f32 * CARD_WIDTH) - (0.5 * PT_TO_MM),
                            global_horizontal_offset + (x as f32 * CARD_WIDTH) + (0.5 * PT_TO_MM),
                            global_vertical_offset + (y as f32 * CARD_HEIGHT) - (0.125 * IN_TO_MM),
                            global_vertical_offset + (y as f32 * CARD_HEIGHT) + (0.125 * IN_TO_MM),
                        ));
                    }
                }
                marks
            }
            CutIndicator::None => vec![],
        }
    }
}
