use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use codee::{Decoder, Encoder};
use futures::{StreamExt, stream::FuturesUnordered};
use leptos::{prelude::*, task::spawn_local};
use leptos_use::storage::use_session_storage;
use nucleo_matcher::{
    Matcher,
    pattern::{CaseMatching, Normalization, Pattern},
};
use printpdf::{
    LinePoint, Mm, Op, PaintMode, PdfDocument, PdfPage, PdfSaveOptions, Point, Polygon,
    PolygonRing, RawImage, WindingOrder, XObjectTransform,
};
use proxy_elev::{
    AlternateFaceMetadata, BleedMode, CardFacePrintingId, CardId, CutIndicator, FilledCardSlot,
    Library, MultiLibrary, PrintConfig, PrintFile, PrintSize,
};
use reactive_stores::{Store, Subfield};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Blob, Url, js_sys::Uint8Array};

static MULTI_LIBRARY: std::sync::LazyLock<MultiLibrary> =
    std::sync::LazyLock::new(proxy_elev::manifest);

fn use_libraries() -> (Signal<Libraries>, WriteSignal<Libraries>) {
    let (get, set, _delete) = use_session_storage::<Libraries, RonSerdeCodec>("libraries-v1");
    (get, set)
}

fn use_print_file() -> (Signal<PrintFile>, WriteSignal<PrintFile>) {
    let (get, set, _delete) = use_session_storage::<PrintFile, RonSerdeCodec>("print-set-v0");
    (get, set)
}

fn use_print_config() -> (Signal<PrintConfig>, WriteSignal<PrintConfig>) {
    let (get, set, _delete) = use_session_storage::<PrintConfig, RonSerdeCodec>("print-config-v0");
    (get, set)
}

#[derive(Debug, Clone, Store)]
pub struct AppState {
    index: Option<usize>,
    tab: Tab,
    printing: bool,
}
fn use_print_index() -> Subfield<Store<AppState>, AppState, Option<usize>> {
    expect_context::<Store<AppState>>().index()
}
fn use_tab() -> Subfield<Store<AppState>, AppState, Tab> {
    expect_context::<Store<AppState>>().tab()
}
fn use_printing() -> Subfield<Store<AppState>, AppState, bool> {
    expect_context::<Store<AppState>>().printing()
}

pub struct RonSerdeCodec;

impl<T: Serialize> Encoder<T> for RonSerdeCodec {
    type Error = ron::Error;
    type Encoded = String;

    fn encode(val: &T) -> Result<Self::Encoded, Self::Error> {
        ron::to_string(val)
    }
}

impl<T> Decoder<T> for RonSerdeCodec
where
    for<'de> T: Deserialize<'de>,
{
    type Error = ron::error::SpannedError;
    type Encoded = str;

    fn decode(val: &Self::Encoded) -> Result<T, Self::Error> {
        ron::from_str(val)
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(Root);
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Libraries {
    loaded_libraries: HashSet<String>,
    library: Library,
}
impl Default for Libraries {
    fn default() -> Libraries {
        let mut base_state = Libraries {
            loaded_libraries: HashSet::new(),
            library: Library {
                cards: HashMap::new(),
                faces: HashMap::new(),
                inserts: HashMap::new(),
            },
        };
        let lib = &MULTI_LIBRARY.libraries["NSG English"];
        base_state.library.merge(lib);
        base_state
            .loaded_libraries
            .insert("NSG English".to_string());
        base_state
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
enum Tab {
    #[default]
    AddCard,
    AddInsert,
    EditCard,
    Print,
    LoadPremadeList,
    ConfigureLibrary,
}
const TABS: &[Tab] = &[Tab::AddCard, Tab::Print];
impl Tab {
    pub fn name(self) -> &'static str {
        match self {
            Tab::AddCard => "Cards",
            Tab::AddInsert => "Inserts",
            Tab::EditCard => "Edit",
            Tab::Print => "Print",
            Tab::LoadPremadeList => "Lists",
            Tab::ConfigureLibrary => "Libraries",
        }
    }
}

#[component]
fn Root() -> impl IntoView {
    provide_context(Store::new(AppState {
        index: None,
        tab: Tab::AddCard,
        printing: false,
    }));
    view! {
        <div class="bg-zinc-900 grid auto-rows-[min-content_1fr_min-content] gap-2 h-screen">
            <div class="bg-zinc-700 p-2 shadow-lg">
                "NRO Proxy Generator"
            </div>
            <div class="p-4 overflow-y-scroll">
                <DecklistView />
            </div>
            <div>
                <ControlConfig />
            </div>
        </div>
    }
}

#[component]
fn DecklistView() -> impl IntoView {
    let (print_file, _) = use_print_file();
    let tab = use_tab();
    let (libraries, _) = use_libraries();
    let print_index = use_print_index();
    let num_items = Memo::new(move |_| print_file.with(PrintFile::len));
    view! {
        <div class="flex flex-wrap gap-2 justify-center">
            <For
                each=move || 0..num_items.get()
                key=|i| *i
                children=move |i| {
                    let card = Memo::new(move |_| {
                        print_file.with(|print_file| {
                            print_file.get(i).cloned()
                        })
                    });
                    let selected = Memo::new(move |_| {
                        print_index.get().is_some_and(|index| index == i) && matches!(tab.get(), Tab::EditCard)
                    });
                    let name = Memo::new(move |_| card.with(|card| {
                        card.as_ref().map(|card| card.name(&libraries.read().library).to_string()).unwrap_or_default()
                    }));
                    let image_url = Memo::new(move |_| card.with(|card| {
                        card.as_ref().map(FilledCardSlot::image_url).unwrap_or_default()
                    }));
                    view! {
                        <button
                            on:click:target=move |_| {
                                print_index.set(Some(i));
                                tab.set(Tab::EditCard);
                            }
                        >
                            <img
                                class:ring-4=selected
                                class="ring-blue-800 w-24"
                                src=image_url
                                alt=name
                            />
                        </button>
                    }
                }
            />
        </div>
    }
}

#[component]
fn ControlConfig() -> impl IntoView {
    let tab = use_tab();

    let name = move || match &*tab.read() {
        Tab::ConfigureLibrary => view! { <Libraries /> }.into_any(),
        Tab::AddCard => view! { <Add /> }.into_any(),
        Tab::EditCard => view! { <Edit /> }.into_any(),
        Tab::Print => view! { <Print /> }.into_any(),
        tab => view! { <p>{tab.name()}</p> }.into_any(),
    };
    let tabs = move || {
        let added_tab = {
            let tab = tab.get();
            (!TABS.contains(&tab)).then_some(tab)
        };
        TABS.iter()
            .copied()
            .chain(added_tab)
            .map(|t| {
                let selected = move || tab.get() == t;
                let not_selected = move || !selected();
                view! {
                    <button
                        class="hover:bg-zinc-600 p-2 rounded-t-xl cursor-pointer"
                        class:bg-zinc-700=selected
                        class:bg-zinc-800=not_selected
                        on:click=move |_| tab.set(t)
                    >
                        {t.name()}
                    </button>
                }
            })
            .collect::<Vec<_>>()
    };
    view! {
        <div class="grid auto-rows-[min-content_1fr] px-2 h-full">
            <div class="grid grid-cols-5 gap-2 bg-zinc-900 max-w-screen-md">{tabs}</div>
            <div class="bg-zinc-700 p-4 min-h-[250px]">{name}</div>
        </div>
    }
}

#[component]
fn Print() -> impl IntoView {
    let (print_config, set_print_config) = use_print_config();
    let printing = use_printing();
    let sizes = [PrintSize::A4, PrintSize::UsLetter];
    let cut_indicators = [CutIndicator::Lines, CutIndicator::Marks, CutIndicator::None];
    let bleed_modes = [
        BleedMode::Borderless,
        BleedMode::Small,
        BleedMode::Medium,
        BleedMode::Large,
    ];
    let is_printing = Memo::new(move |_| printing.get());
    let is_not_printing = Memo::new(move |_| !is_printing.get());
    let print_message = Memo::new(move |_| {
        if is_printing.get() {
            "Generating..."
        } else {
            "Generate PDF"
        }
    });
    view! {
        <div class="flex flex-col gap-2 h-full justify-between">
            <div class="flex gap-2 items-center flex-wrap">
                <div class="font-bold">Paper Size</div>
                <For
                    each=move || sizes
                    key=|size| *size
                    children=move |size| {
                        let selected = Memo::new(move |_| {
                            print_config.with(|print_config| print_config.print_size == size)
                        });
                        let not_selected = Memo::new(move |_| !selected.get());
                        view! {
                            <button
                                class="p-2 rounded-lg cursor-pointer"
                                class:bg-blue-800=selected
                                class:hover:bg-zinc-600=not_selected
                                class:bg-zinc-800=not_selected
                                on:click:target=move |_| {
                                    set_print_config.update(move |config| config.print_size = size);
                                }
                            >
                                {format!("{size}")}
                            </button>
                        }
                    }
                />
            </div>
            <div class="flex gap-2 items-center flex-wrap">
                <div class="font-bold">Cut Indicator</div>
                <For
                    each=move || cut_indicators
                    key=|cut| *cut
                    children=move |cut| {
                        let selected = Memo::new(move |_| {
                            print_config.with(|print_config| print_config.cut_indicator == cut)
                        });
                        let not_selected = Memo::new(move |_| !selected.get());
                        view! {
                            <button
                                class="p-2 rounded-lg cursor-pointer"
                                class:bg-blue-800=selected
                                class:hover:bg-zinc-600=not_selected
                                class:bg-zinc-800=not_selected
                                on:click:target=move |_| {
                                    set_print_config.update(move |config| config.cut_indicator = cut);
                                }
                            >
                                {format!("{cut}")}
                            </button>
                        }
                    }
                />
            </div>
            <div class="flex gap-2 items-center flex-wrap">
                <div class="font-bold">Bleed Mode</div>
                <For
                    each=move || bleed_modes
                    key=|bleed| *bleed
                    children=move |bleed| {
                        let selected = Memo::new(move |_| {
                            print_config.with(|print_config| print_config.bleed_mode == bleed)
                        });
                        let not_selected = Memo::new(move |_| !selected.get());
                        view! {
                            <button
                                class="p-2 rounded-lg cursor-pointer"
                                class:bg-blue-800=selected
                                class:hover:bg-zinc-600=not_selected
                                class:bg-zinc-800=not_selected
                                on:click:target=move |_| {
                                    set_print_config.update(move |config| config.bleed_mode = bleed);
                                }
                            >
                                {format!("{bleed}")}
                            </button>
                        }
                    }
                />
            </div>
            <div>
                <button
                    class="bg-green-800 hover:bg-green-600 p-2 rounded-lg cursor-pointer font-bold text-lg"
                    class:bg-green-800=is_not_printing
                    class:hover:bg-green-600=is_not_printing
                    class:bg-red-800=is_printing
                    disabled=is_printing
                    on:click:target=move |_| {
                        do_print(printing);
                    }
                >
                    {print_message}
                </button>
            </div>
        </div>
    }
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::cast_possible_truncation)]
fn do_print(printing: Subfield<Store<AppState>, AppState, bool>) {
    printing.set(true);
    let (print_file, _) = use_print_file();
    let (print_config, _) = use_print_config();
    let print_file = print_file.read();
    let print_config = print_config.get();

    spawn_local(async move {
        let mut doc = PdfDocument::new("proxies");
        let files_to_download = print_file
            .all()
            .iter()
            .map(FilledCardSlot::image_url)
            .collect::<HashSet<_>>();
        let downloaded_files = files_to_download
            .into_iter()
            .map(|url| async move {
                let bytes = reqwest::get(&url)
                    .await
                    .expect("Cannot Download")
                    .bytes()
                    .await
                    .expect("Cannot get bytes");
                let image =
                    RawImage::decode_from_bytes(&bytes, &mut vec![]).expect("cannot decode");
                (url, image)
            })
            .collect::<FuturesUnordered<_>>()
            .collect::<HashMap<String, RawImage>>()
            .await;

        let mut page_ops: Vec<Vec<Op>> = vec![vec![]; print_file.all().len().div_ceil(9)];
        let transforms = (0..9)
            .map(|slot| {
                let (x, y, scale) = print_config.slot(slot);
                XObjectTransform {
                    translate_x: Some(Mm(x).into()),
                    translate_y: Some(Mm(y).into()),
                    scale_x: Some(scale),
                    scale_y: Some(scale),
                    dpi: Some(300.0),
                    ..Default::default()
                }
            })
            .collect::<Vec<_>>();
        let marks = print_config
            .marks()
            .into_iter()
            .map(|(x1, x2, y1, y2)| Op::DrawPolygon {
                polygon: Polygon {
                    rings: vec![PolygonRing {
                        points: vec![
                            LinePoint {
                                p: Point::new(Mm(x1), Mm(y1)),
                                bezier: false,
                            },
                            LinePoint {
                                p: Point::new(Mm(x2), Mm(y1)),
                                bezier: false,
                            },
                            LinePoint {
                                p: Point::new(Mm(x2), Mm(y2)),
                                bezier: false,
                            },
                            LinePoint {
                                p: Point::new(Mm(x1), Mm(y2)),
                                bezier: false,
                            },
                        ],
                    }],
                    mode: PaintMode::Fill,
                    winding_order: WindingOrder::NonZero,
                },
            })
            .collect::<Vec<_>>();
        for (i, slot) in print_file.all().iter().enumerate() {
            let page_index = (i + 1).div_ceil(9) - 1;
            let page_slot = i % 9;
            let url = slot.image_url();
            let id = doc.add_image(&downloaded_files[&url]);
            let object = Op::UseXobject {
                id,
                transform: transforms[page_slot],
            };
            page_ops[page_index].push(object);
        }
        for page in &mut page_ops {
            page.extend(marks.clone());
        }

        let (page_width, page_height) = print_config.paper();
        let pages = page_ops
            .into_iter()
            .map(|ops| PdfPage::new(Mm(page_width), Mm(page_height), ops))
            .collect();
        let pdf_bytes = doc
            .with_pages(pages)
            .save(&PdfSaveOptions::default(), &mut vec![]);
        let js_bytes = Uint8Array::new_with_length(pdf_bytes.len() as u32);
        js_bytes.copy_from(&pdf_bytes);
        let js_array = JsValue::from(Box::new([js_bytes]) as Box<[_]>);
        let js_bytes_blob = Blob::new_with_buffer_source_sequence(&js_array).expect("blob");
        let link = document()
            .create_element("a")
            .expect("element")
            .dyn_into::<web_sys::HtmlAnchorElement>()
            .expect("anchor");
        let url = Url::create_object_url_with_blob(&js_bytes_blob).expect("url");
        link.set_href(&url);
        link.set_download("proxies.pdf");
        let body = document().body().expect("body");
        let cld = body.append_child(&link).expect("append");
        link.click();
        body.remove_child(&cld).expect("remove");
        Url::revoke_object_url(&url).expect("revoke");
        printing.set(false);
    });
}

#[component]
fn Edit() -> impl IntoView {
    let (print_file, set_print_file) = use_print_file();
    let (libraries, _) = use_libraries();
    let print_index = use_print_index();

    let card = Memo::new(move |_| {
        let print_index = print_index.get();
        print_file.with(|print_file| print_index.and_then(|index| print_file.get(index).cloned()))
    });

    let name = Memo::new(move |_| {
        let libraries = libraries.read();
        card.with(|card| {
            card.as_ref()
                .map(|card| card.name(&libraries.library).to_string())
                .unwrap_or_default()
        })
    });

    let face_info = Memo::new(move |_| {
        let Some(FilledCardSlot::Card {
            printing:
                face_id @ CardFacePrintingId {
                    face_or_variant_specifier: Some(face),
                    ..
                },
        }) = card.get()
        else {
            return None;
        };
        let libraries = libraries.read();
        let card_data = libraries.library.get_face_card(&face_id);

        let faces = match &card_data.alternate_face_data {
            AlternateFaceMetadata::Single => return None,
            AlternateFaceMetadata::Multiple(titles) => titles.len() + 1,
            &AlternateFaceMetadata::Variants(n) => n,
        };
        Some((face, face_id, faces))
    });

    let face_edit = move || {
        let Some((face, face_id, faces)) = face_info.get() else {
            return ().into_any();
        };

        view! {
            <div class="flex gap-2">
                <For
                    each=move || 1..=faces
                    key=|i| *i
                    children=move |button_face| {
                        let face_id = face_id.clone();
                        let selected = button_face == face;
                        let libraries = libraries.read();
                        view! {
                            <button
                                class="hover:bg-zinc-600 p-2 rounded-lg cursor-pointer"
                                class:bg-blue-800=selected
                                class:bg-zinc-800=!selected
                                on:click:target=move |_| {
                                    let mut new = face_id.clone();
                                    new.face_or_variant_specifier = Some(button_face);
                                    if let Some(index) = print_index.get() {
                                        set_print_file.write().update_card(index, new, &libraries.library);
                                    }
                                }
                            >
                                {"Face "} {button_face}
                            </button>
                        }
                    }
                />
            </div>
        }
        .into_any()
    };

    view! {
        <div class="flex flex-col gap-2">
            <div class="text-lg font-bold">{name}</div>
            <BasicEdit />
            {face_edit}
        </div>
    }
}

#[component]
fn BasicEdit() -> impl IntoView {
    let print_index = use_print_index();
    let tab = use_tab();
    view! {
        <div class="flex gap-2">
            <button
                class="bg-red-800 hover:bg-red-600 p-2 rounded-lg cursor-pointer"
                on:click:target=move |_| {
                    let (libraries, _) = use_libraries();
                    let (_, set_print_file) = use_print_file();
                    let library = libraries.get().library;
                    if let Some(index) = print_index.get() {
                        set_print_file.write().remove_card(index, &library);
                    };
                    tab.set(Tab::AddCard);
                }
            >
                {"Remove"}
            </button>
        </div>
    }
}

// bg-zinc-700
// ring-4
// bg-zinc-800
// bg-blue-800

#[derive(PartialEq, Debug, Clone)]
struct Haystack {
    haystack: Vec<String>,
    mappings: HashMap<String, CardId>,
}

#[component]
fn Add() -> impl IntoView {
    let (libraries, _set_libraries) = use_libraries();
    let (_, set_print_file) = use_print_file();
    let mut matcher_config = nucleo_matcher::Config::DEFAULT;
    matcher_config.ignore_case = true;
    matcher_config.normalize = true;
    matcher_config.prefer_prefix = true;
    let matcher = Arc::new(Mutex::new(Matcher::new(matcher_config)));

    let (input, set_input) = signal(String::new());

    let haystack = Memo::new(move |_| {
        let mut haystack = Haystack {
            haystack: vec![],
            mappings: HashMap::new(),
        };
        for (card, meta) in &libraries.read().library.cards {
            haystack.haystack.push(meta.title.title.clone());
            haystack.haystack.push(meta.title.stripped_title.clone());
            haystack
                .mappings
                .insert(meta.title.title.clone(), card.clone());
            haystack
                .mappings
                .insert(meta.title.stripped_title.clone(), card.clone());
        }
        haystack
    });

    let found = Memo::new(move |_| {
        let haystack = haystack.read();
        let pattern = Pattern::parse(&input.get(), CaseMatching::Ignore, Normalization::Smart);
        let out = pattern.match_list(&haystack.haystack, &mut matcher.lock().unwrap());
        let mut found = HashSet::new();
        let mut olist = Vec::with_capacity(5);
        for (entry, _) in out {
            let card = &haystack.mappings[entry];
            if found.insert(card) {
                olist.push(card.clone());
            }
            if found.len() == 5 {
                break;
            }
        }
        olist
    });

    view! {
        <input
            type="text"
            class="bg-zinc-900 border-1 border-white p-2 rounded-md w-full"
            on:input:target=move |ev| {
            // .value() returns the current value of an HTML input element
                set_input.set(ev.target().value());
            }
            on:keydown=move |key| {
                if key.key() == "Enter" {
                    if let Some(found) = found.read().first() {
                        let libraries = libraries.read();
                        let card = libraries.library.get_card(found);
                        set_print_file.write().add_cards(card);
                    }
                }
            }
            prop:value=input
        />
        <For
            each=move || found.get()
            key=|card| card.clone()
            children=move |card| {
                let libraries = libraries.read();
                let card = libraries.library.get_card(&card);
                view! { <div>{card.title.title.clone()}</div> }
            }
        />
    }
}

#[component]
fn Libraries() -> impl IntoView {
    let (libraries, _set_libraries) = use_libraries();
    let libraries = move || {
        MULTI_LIBRARY
            .libraries
            .keys()
            .map(|name| {
                let loaded = libraries.read().loaded_libraries.contains(name);
                view! {
                    <button
                        class="p-2 cursor-pointer rounded-lg"
                        class:bg-blue-800=loaded
                        class:bg-zinc-800=!loaded
                    >
                        {name.as_str()}
                    </button>
                }
            })
            .collect::<Vec<_>>()
    };
    view! {
        <div>
            {libraries}
        </div>
    }
}
