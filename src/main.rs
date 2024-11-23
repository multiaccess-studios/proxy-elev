use codee::string::JsonSerdeCodec;
use futures::{stream::FuturesUnordered, StreamExt};
use html::body;
use leptos::*;
use leptos_dom::logging::console_log;
use leptos_use::storage::{use_local_storage, use_local_storage_with_options, UseStorageOptions};
use nucleo_matcher::{
    pattern::{CaseMatching, Normalization, Pattern},
    Config, Matcher,
};
use printpdf::{
    image::RawImage, Color, Mm, Op, PaintMode, PdfDocument, PdfPage, PdfSaveOptions, Point,
    Polygon, Rgb, WindingOrder, XObjectTransform,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{js_sys::Uint8Array, Blob, Url};

const DB_URL: &'static str =
    "https://nr-card-printings-7cc83f6c-d908-4c99-8c9f-5018927c1533.s3.eu-west-2.amazonaws.com";

const NAMES_INDEX: &'static str = include_str!("../names.index.json");
const NRDBID_INDEX: &'static str = include_str!("../nrdbid.index.json");

fn card_url(cycle: &str, index: u32) -> String {
    format!("{DB_URL}/{cycle}/{index:>03}.webp")
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
struct AppState {
    cards: BTreeMap<usize, CardRecord>,
    next_card: usize,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
struct LayoutConfig {
    pw: String,
    ph: String,
    pmt: String,
    pmr: String,
    pmb: String,
    pml: String,
    xdr: String,
    chm: String,
    cwm: String,
    cb: String,
}
impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            pw: "210.0".into(),
            ph: "297.0".into(),
            pmt: "5.0".into(),
            pmr: "5.0".into(),
            pmb: "5.0".into(),
            pml: "5.0".into(),
            xdr: "2.0".into(),
            chm: "86.9".into(),
            cwm: "61.5".into(),
            cb: "2.0".into(),
        }
    }
}
impl LayoutConfig {
    fn parse(&self) -> ParsedLayoutConfig {
        ParsedLayoutConfig {
            pw: self
                .pw
                .parse()
                .unwrap_or_else(|_| LayoutConfig::default().pw.parse().unwrap()),
            ph: self
                .ph
                .parse()
                .unwrap_or_else(|_| LayoutConfig::default().ph.parse().unwrap()),
            pmt: self
                .pmt
                .parse()
                .unwrap_or_else(|_| LayoutConfig::default().pmt.parse().unwrap()),
            pmr: self
                .pmr
                .parse()
                .unwrap_or_else(|_| LayoutConfig::default().pmr.parse().unwrap()),
            pmb: self
                .pmb
                .parse()
                .unwrap_or_else(|_| LayoutConfig::default().pmb.parse().unwrap()),
            pml: self
                .pml
                .parse()
                .unwrap_or_else(|_| LayoutConfig::default().pml.parse().unwrap()),
            xdr: self
                .xdr
                .parse()
                .unwrap_or_else(|_| LayoutConfig::default().xdr.parse().unwrap()),
            chm: self
                .chm
                .parse()
                .unwrap_or_else(|_| LayoutConfig::default().chm.parse().unwrap()),
            cwm: self
                .cwm
                .parse()
                .unwrap_or_else(|_| LayoutConfig::default().cwm.parse().unwrap()),
            cb: self
                .cb
                .parse()
                .unwrap_or_else(|_| LayoutConfig::default().cb.parse().unwrap()),
        }
    }
}

struct ParsedLayoutConfig {
    pw: f32,
    ph: f32,
    pmt: f32,
    pmr: f32,
    pmb: f32,
    pml: f32,
    xdr: f32,
    chm: f32,
    cwm: f32,
    cb: f32,
}
impl ParsedLayoutConfig {
    // x, y, width, height
    fn safe_region(&self) -> (f32, f32, f32, f32) {
        (
            self.pml,
            self.ph - self.pmt,
            self.pw - self.pml - self.pmr,
            self.ph - self.pmt - self.pmb,
        )
    }
    // number of columns, number of rows
    fn layout(&self) -> (f32, f32) {
        let (_, _, width, height) = self.safe_region();
        (
            ((width - self.xdr) / self.cwm).floor(),
            ((height - self.xdr) / self.chm).floor(),
        )
    }

    // x, y, width, height
    fn extent_region(&self) -> (f32, f32, f32, f32) {
        let (mx, my, mw, mh) = self.safe_region();

        let (lx, ly) = self.layout();

        let lw = lx * self.cwm + self.xdr;
        let lh = ly * self.chm + self.xdr;

        let upx = mw - lw;
        let upy = mh - lh;

        (mx + upx / 2.0, my - upy / 2.0, lw, lh)
    }

    // x, y, width, height
    fn card_safe_area(&self, card: usize) -> (f32, f32, f32, f32) {
        let (lx, ly) = self.layout();
        let lx = lx as usize;
        let ly = ly as usize;
        let card = card % (lx * ly);
        let px = card % lx;
        let py = card / lx;
        let (ex, ey, _, _) = self.extent_region();

        let c0x = ex + (self.xdr / 2.0) + self.cb;
        let c0y = ey - (self.xdr / 2.0) - self.cb;

        let cx = c0x + (px as f32 * self.cwm);
        let cy = c0y - (py as f32 * self.chm);
        (
            cx,
            cy,
            self.cwm - self.cb - self.cb,
            self.chm - self.cb - self.cb,
        )
    }

    // x, y, and scale
    fn card_area(&self, card: usize, cw: f32, ch: f32) -> (f32, f32, f32) {
        let (sx, sy, sw, sh) = self.card_safe_area(card);
        let scale = (sw / cw).min(sh / ch);
        let acw = cw * scale;
        let ach = ch * scale;
        let upx = sw - acw;
        let upy = sh - ach;
        (sx + upx / 2.0, (sy - ach - upy / 2.0), scale)
    }

    // x1, x2, y1, y2
    fn cut_mark_points(&self, cut: usize) -> (f32, f32, f32, f32) {
        let (ex, ey, _, _) = self.extent_region();
        let (lx, _) = self.layout();
        let cx = lx as usize + 1;
        let xx = cut % cx;
        let xy = cut / cx;

        let ox = (xx as f32) * self.cwm;
        let oy = (xy as f32) * self.chm;

        (ex + ox, ex + ox + self.xdr, ey - oy, ey - oy - self.xdr)
    }
}

#[derive(Clone)]
struct MatcherState {
    matcher: Matcher,
    matches: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
struct CardRecord {
    name: String,
    printing: (String, u32),
}

fn main() {
    mount_to_body(|| view! { <App/> })
}

#[component]
fn App() -> impl IntoView {
    let mut config = Config::DEFAULT;

    config.normalize = true;
    config.ignore_case = true;
    config.prefer_prefix = true;
    let (matcher, set_matcher) = create_signal(MatcherState {
        matcher: Matcher::new(config),
        matches: vec![],
    });

    let names_index: HashMap<String, Vec<(String, u32)>> =
        serde_json::from_str(&NAMES_INDEX).expect("deser");
    let nrdbid_index: HashMap<String, String> = serde_json::from_str(&NRDBID_INDEX).expect("deser");
    let nrdbid_index: &'static HashMap<String, String> = Box::leak(Box::new(nrdbid_index));
    let names_index: &'static HashMap<String, Vec<(String, u32)>> =
        Box::leak(Box::new(names_index));

    let decklist_regex =
        Regex::new(r#"^(https?:\/\/)(.+?)\/(?:.+?)\/(deck(?:list)?)(?:\/view)?\/([a-f0-9\-]+).*$"#)
            .unwrap();
    let decklist_regex_replace = r#"$1$2/api/2.0/public/$3/$4"#;

    let mut name_list: Vec<String> = names_index.keys().map(|k| k.to_string()).collect();
    name_list.sort();
    let name_list: &'static [String] = Box::leak(name_list.into_boxed_slice());

    let (state, set_state, delete_state) =
        use_local_storage::<AppState, JsonSerdeCodec>("proxy_nro_app_state_2024_11_22");
    let (input, set_input) = create_signal("".to_string());
    let (nrdb_input, set_nrdb_input) = create_signal("".to_string());

    let mut printing_alias: HashMap<_, _> = HashMap::new();
    printing_alias.insert("kitara", "Kit");
    printing_alias.insert("red_sand", "RS");
    printing_alias.insert("reign_and_reverie", "R&R");
    printing_alias.insert("magnum_opus_reprint", "MO");
    printing_alias.insert("ashes", "Ash");
    printing_alias.insert("system_gateway", "SG");
    printing_alias.insert("system_update_2021", "SU21");
    printing_alias.insert("borealis", "Bor");
    printing_alias.insert("liberation", "Lib");
    let printing_alias: &'static HashMap<_, _> = Box::leak(Box::new(printing_alias));

    let (layout_config, set_layout_config, delete_layout_config) =
        use_local_storage_with_options::<LayoutConfig, JsonSerdeCodec>(
            "proxy_nro_layout_config_2024_11_22",
            UseStorageOptions::default().initial_value(LayoutConfig::default()),
        );

    create_effect(move |_| {
        set_matcher.update(|matcher| {
            matcher.matches =
                Pattern::parse(&input.get(), CaseMatching::Ignore, Normalization::Smart)
                    .match_list(name_list, &mut matcher.matcher)
                    .iter()
                    .map(|n| n.0.clone())
                    .take(10)
                    .collect();
        })
    });

    let (downloading, set_downloading) = create_signal(false);
    let (dl_len, set_dl_len) = create_signal(100);
    let (dl_done, set_dl_done) = create_signal(0);

    let (importing, set_importing) = create_signal(false);

    let (show_config, set_show_config) = create_signal(false);

    view! {
        <div id="input-box">
            <form
                id="csel"
                on:submit=move |ev| {
                    ev.prevent_default();
                    let cname = ev.submitter()
                        .expect("submitter")
                        .dyn_into::<web_sys::HtmlInputElement>()
                        .expect("input element")
                        .name();
                    set_state.update(|state| {
                        state.cards.insert(state.next_card, CardRecord {
                            name: cname.clone(),
                            printing: names_index[&cname][0].clone()
                        });
                        state.next_card += 1;
                    });
                }
            >
                <div id="tin">
                    <label id="cin">
                        <span>"Enter Card Name"</span>
                        <input
                            type="text"
                            on:input=move |ev| {
                                set_input.set(event_target_value(&ev))
                            }
                            prop:value=input
                        />
                    </label>
                    <input
                        type="submit"
                        value="Add Card"
                        prop:name=move || matcher.get().matches.get(0).cloned().unwrap_or_default()
                    />
                    </div>
                    <div id="clist">
                    <For
                        each=move || matcher.get().matches
                        key=|c| c.to_string()
                        children=move |c| {
                            view! {
                                <input
                                    type="submit"
                                    value=c.clone()
                                    name=c
                                    class="choice"
                                    role="button"
                                    tabindex="0"
                                />
                            }
                        }
                    />
                </div>
            </form>
            <form
                id="nrsel"
                on:submit=move |ev| {
                    ev.prevent_default();
                    set_importing.set(true);
                    let inp = nrdb_input.get();
                    if decklist_regex.is_match(&inp) {
                        let url = decklist_regex.replace_all(&inp, decklist_regex_replace).to_string();
                        spawn_local(async move {
                            console_log("Reading decklist");
                            let decklist: Value = reqwest::get(&url).await.unwrap().json().await.unwrap();
                            console_log("Read");
                            let cards = decklist["data"][0]["cards"].as_object().unwrap();
                            console_log("Updating");
                            set_state.update(|state| {
                                for (card, count) in cards {
                                    let count = count.as_number().unwrap();
                                    let count = count.as_u64().unwrap();
                                    let card = &nrdbid_index[&*card];
                                    for _ in 0..count {
                                        state.cards.insert(state.next_card, CardRecord {
                                            name: card.clone(),
                                            printing: names_index[&*card][0].clone(),
                                        });
                                        state.next_card += 1;
                                    }
                                }
                            });
                            set_importing.set(false);
                        })
                    }
                }
            >
                <div id="nrin">
                    <label id="nrtin">
                        <span>"Enter NetrunnerDB URL"</span>
                        <input
                            type="text"
                            on:input=move |ev| {
                                set_nrdb_input.set(event_target_value(&ev))
                            }
                            prop:value=nrdb_input
                        />
                    </label>
                    <input
                        type="submit"
                        value="Import"
                        prop:disabled=move || importing.get().then_some("disabled")
                    />
                </div>
            </form>
            <button
                id="download"
                on:click=move |ev| {
                    ev.prevent_default();
                    set_downloading.set(true);
                    let mut dls = HashSet::new();
                    let mut writes = Vec::new();
                    for (_, card) in state.get().cards {
                        let &(ref root, num) = &card.printing;
                        let url = card_url(root, num);
                        dls.insert(url.clone());
                        writes.push(url);
                    }
                    set_dl_len.set(dls.len() * 3);
                    let mut dl_all = dls
                        .into_iter()
                        .map(|dl| async move {
                            let bytes = reqwest::get(&dl)
                                .await
                                .expect("cannot download")
                                .bytes()
                                .await
                                .expect("not bytes");
                            set_dl_done.update(|dl_done| *dl_done += 1);
                            let image = RawImage::decode_from_bytes(&bytes).expect("cannot decode");
                            set_dl_done.update(|dl_done| *dl_done += 1);
                            (dl, image)
                        }).collect::<FuturesUnordered<_>>();
                    spawn_local(async move {
                        let mut doc = PdfDocument::new("proxies");
                        let mut immap = HashMap::new();
                        let layout_config = layout_config.get_untracked().parse();
                        let (lx, ly) = layout_config.layout();
                        let cards_per_page = (lx * ly) as usize;
                        let cuts_marks_per_page = ((lx + 1.0) * (ly + 1.0)) as usize;
                        let mut pages_needed = writes.len() / cards_per_page;
                        if writes.len() % cards_per_page > 0 {
                            pages_needed += 1;
                        }

                        while let Some((url, dl)) = dl_all.next().await {
                            immap.insert(url, (dl.width, dl.height, doc.add_image(&dl)));
                            set_dl_done.update(|dl_done| *dl_done += 1);
                        }
                        let mut pages = Vec::new();
                        for page in 0..pages_needed {
                            let mut page_content = vec![];
                            for card in 0..cards_per_page {
                                let global_card_count = page * cards_per_page + card;
                                if global_card_count >= writes.len() {
                                    break;
                                }
                                let write = &writes[global_card_count];
                                let (imw, imh, xid) = &immap[write];
                                let iw = (*imw as f32) / 11.811;
                                let ih = (*imh as f32) / 11.811;
                                let (px, py, s) = layout_config.card_area(card, iw, ih);
                                let mut transform = XObjectTransform::default();
                                transform.translate_x = Some(Mm(px).into());
                                transform.translate_y = Some(Mm(py).into());
                                transform.scale_x = Some(s);
                                transform.scale_y = Some(s);
                                transform.dpi = Some(300.0);
                                page_content.push(Op::UseXObject {
                                    id: xid.clone(),
                                    transform
                                });
                            }
                            for cut_mark in 0..cuts_marks_per_page {
                                let (x1, x2, y1, y2) = layout_config.cut_mark_points(cut_mark);
                                page_content.push(Op::DrawPolygon {
                                    polygon: Polygon {
                                        rings: vec![vec![
                                            (Point::new(Mm(x1), Mm(y1)), false),
                                            (Point::new(Mm(x2), Mm(y1)), false),
                                            (Point::new(Mm(x2), Mm(y2)), false),
                                            (Point::new(Mm(x1), Mm(y2)), false),
                                        ]],
                                        mode: PaintMode::Fill,
                                        winding_order: WindingOrder::NonZero
                                    }
                                });
                            }
                            let page = PdfPage::new(Mm(layout_config.pw), Mm(layout_config.ph), page_content);
                            pages.push(page);
                        }
                        let pdf_bytes = doc.with_pages(pages).save(&PdfSaveOptions::default());
                        let js_bytes = Uint8Array::new_with_length(pdf_bytes.len() as u32);
                        js_bytes.copy_from(&pdf_bytes);
                        let js_array = JsValue::from(Box::new([js_bytes]) as Box<[_]>);
                        let js_bytes_blob = Blob::new_with_buffer_source_sequence(&js_array)
                            .expect("blob");
                        let link = document().create_element("a").expect("element")
                            .dyn_into::<web_sys::HtmlAnchorElement>().expect("anchor");
                        let url = Url::create_object_url_with_blob(&js_bytes_blob).expect("url");
                        link.set_href(&url);
                        link.set_download("proxies.pdf");
                        let body = body();
                        let cld = body.append_child(&link).expect("append");
                        link.click();
                        body.remove_child(&cld).expect("remove");
                        Url::revoke_object_url(&url).expect("revoke");
                        set_downloading.set(false);
                    });

                }
                prop:disabled=move || downloading.get().then_some("disabled")
            >
                <span>"Generate PDF"</span>
                <progress
                    max=move || dl_len.get()
                    value=move || dl_done.get()
                >
                </progress>
            </button>
            <button
                id="clear"
                on:click=move |_ev| {
                    set_state.update(|state| {
                        state.cards.clear();
                        state.next_card = 0;
                    });
                }
            >
                "Clear All Cards"
            </button>
            {
                move || if !show_config.get() {
                    view! {
                        <>
                            <button
                                on:click=move |_| set_show_config.set(true)
                            >
                                "Show Config"
                            </button>
                        </>
                    }
                } else {
                    view! {
                        <>
                            <button
                                on:click=move |_| set_show_config.set(false)
                            >
                                "Hide Config"
                            </button>
                            <button
                                on:click=move |_| set_layout_config.update(|c| {
                                    c.pw = "210.0".into();
                                    c.ph = "297.0".into();
                                })
                            >
                                "Set A4 Paper"
                            </button>
                            <button
                                on:click=move |_| set_layout_config.update(|c| {
                                    c.pw = "215.9".into();
                                    c.ph = "279.4".into();
                                })
                            >
                                "Set Letter Paper"
                            </button>
                            <div id="cfgopts">
                                <div class="cfgopt">
                                    <label>
                                        <span>"Page Width (mm)"</span>
                                        <input
                                            type="text"
                                            prop:value=move || layout_config.get().pw
                                            on:input=move |ev| set_layout_config.update(|lc| {
                                                lc.pw = event_target_value(&ev);
                                            })
                                        />
                                    </label>
                                </div>
                                <div class="cfgopt">
                                    <label>
                                        <span>"Page Height (mm)"</span>
                                        <input
                                            type="text"
                                            prop:value=move || layout_config.get().ph
                                            on:input=move |ev| set_layout_config.update(|lc| {
                                                lc.ph = event_target_value(&ev);
                                            })
                                        />
                                    </label>
                                </div>
                                <div class="cfgopt">
                                    <label>
                                        <span>"Page Margin Top (mm)"</span>
                                        <input
                                            type="text"
                                            prop:value=move || layout_config.get().pmt
                                            on:input=move |ev| set_layout_config.update(|lc| {
                                                lc.pmt = event_target_value(&ev);
                                            })
                                        />
                                    </label>
                                </div>
                                <div class="cfgopt">
                                    <label>
                                        <span>"Page Margin Right (mm)"</span>
                                        <input
                                            type="text"
                                            prop:value=move || layout_config.get().pmr
                                            on:input=move |ev| set_layout_config.update(|lc| {
                                                lc.pmr = event_target_value(&ev);
                                            })
                                        />
                                    </label>
                                </div>
                                <div class="cfgopt">
                                    <label>
                                        <span>"Page Margin Bottom (mm)"</span>
                                        <input
                                            type="text"
                                            prop:value=move || layout_config.get().pmb
                                            on:input=move |ev| set_layout_config.update(|lc| {
                                                lc.pmb = event_target_value(&ev);
                                            })
                                        />
                                    </label>
                                </div>
                                <div class="cfgopt">
                                    <label>
                                        <span>"Page Margin Left (mm)"</span>
                                        <input
                                            type="text"
                                            prop:value=move || layout_config.get().pml
                                            on:input=move |ev| set_layout_config.update(|lc| {
                                                lc.pml = event_target_value(&ev);
                                            })
                                        />
                                    </label>
                                </div>
                                <div class="cfgopt">
                                    <label>
                                        <span>"Cut Mark Size (mm)"</span>
                                        <input
                                            type="text"
                                            prop:value=move || layout_config.get().xdr
                                            on:input=move |ev| set_layout_config.update(|lc| {
                                                lc.xdr = event_target_value(&ev);
                                            })
                                        />
                                    </label>
                                </div>
                                <div class="cfgopt">
                                    <label>
                                        <span>"Card Maximum Height (mm)"</span>
                                        <input
                                            type="text"
                                            prop:value=move || layout_config.get().chm
                                            on:input=move |ev| set_layout_config.update(|lc| {
                                                lc.chm = event_target_value(&ev);
                                            })
                                        />
                                    </label>
                                </div>
                                <div class="cfgopt">
                                    <label>
                                        <span>"Card Maximum Width (mm)"</span>
                                        <input
                                            type="text"
                                            prop:value=move || layout_config.get().cwm
                                            on:input=move |ev| set_layout_config.update(|lc| {
                                                lc.cwm = event_target_value(&ev);
                                            })
                                        />
                                    </label>
                                </div>
                                <div class="cfgopt">
                                    <label>
                                        <span>"Card Bleed (mm)"</span>
                                        <input
                                            type="text"
                                            prop:value=move || layout_config.get().cb
                                            on:input=move |ev| set_layout_config.update(|lc| {
                                                lc.cb = event_target_value(&ev);
                                            })
                                        />
                                    </label>
                                </div>

                            </div>
                        </>
                    }
                }
            }
        </div>
        <div id="cshow">
            <For
                each=move || state.get().cards
                key=|(i, _)| *i
                children=move |(i, c)| {
                    view! {
                        <div class="csr">{c.name.clone()}</div>
                        <div class="csb">
                            <button
                                class="remove"
                                on:click=move |_| {
                                    set_state.update(|state| {
                                        state.cards.remove(&i);
                                    });
                                }
                            >
                                "⨉"
                            </button>
                            <For
                                each=move || names_index[&c.name.clone()].iter().enumerate()
                                key=|(i, _)| *i
                                children=move |(_, printing)| {
                                    let c_printing = c.printing.clone();
                                    view! {
                                        <button
                                            class="printing"
                                            class:active=move || printing == &c_printing
                                            on:click=move |_ev| {
                                                set_state.update(|state| {
                                                    if let Some(card) = state.cards.get_mut(&i) {
                                                        card.printing = printing.clone();
                                                    }
                                                });
                                            }
                                        >
                                            {printing_alias[&printing.0[..]]}
                                        </button>
                                    }
                                }
                            />
                        </div>
                    }
                }
            />
        </div>
    }
}
