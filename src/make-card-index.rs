use std::{collections::HashMap, error::Error};

use serde_json::Value;

const SUPPORTED_CYCLES: [&'static str; 9] = [
    "kitara",
    "red_sand",
    "reign_and_reverie",
    "magnum_opus_reprint",
    "ashes",
    "system_gateway",
    "system_update_2021",
    "borealis",
    "liberation",
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut names_index: HashMap<_, Vec<_>> = HashMap::new();
    let mut nrdbid_index = HashMap::new();

    for cycle in SUPPORTED_CYCLES {
        println!("Processing {cycle}");
        let get_cycle =
            format!("https://api-preview.netrunnerdb.com/api/v3/public/card_cycles/{cycle}");

        let cycle_data = reqwest::get(get_cycle).await?.json::<Value>().await?;

        let get_cycle_cards = cycle_data["data"]["relationships"]["cards"]["links"]["related"]
            .as_str()
            .ok_or_else(|| "Not string")?;
        let cards = reqwest::get(get_cycle_cards).await?.json::<Value>().await?;
        let cards = cards["data"].as_array().ok_or_else(|| "Not array")?;
        for card in cards {
            let title = card["attributes"]["title"]
                .as_str()
                .ok_or_else(|| "Not str")?
                .to_string();
            let get_printings = card["attributes"]["printing_ids"]
                .as_array()
                .ok_or_else(|| "Not Array")?;
            for printing in get_printings {
                let printing = printing.as_str().ok_or_else(|| "Not String")?.to_string();
                nrdbid_index.insert(printing, title.clone());
            }
        }

        let get_cycle_printings = cycle_data["data"]["relationships"]["printings"]["links"]
            ["related"]
            .as_str()
            .ok_or_else(|| "Not string")?;
        let printings = reqwest::get(get_cycle_printings)
            .await?
            .json::<Value>()
            .await?;
        let printings = printings["data"].as_array().ok_or_else(|| "Not array")?;

        for printing in printings {
            let set_id = printing["attributes"]["card_set_id"]
                .as_str()
                .ok_or_else(|| "Not str")?
                .to_string();
            if set_id.contains("booster_pack") {
                continue;
            }
            let title = printing["attributes"]["title"]
                .as_str()
                .ok_or_else(|| "Not str")?
                .to_string();

            let position = printing["attributes"]["position"]
                .as_u64()
                .ok_or_else(|| "Not number")?;
            names_index
                .entry(title)
                .or_default()
                .push((cycle, position));
        }
    }

    let names_index = serde_json::to_string(&names_index)?;
    let nrdbid_index = serde_json::to_string(&nrdbid_index)?;
    tokio::fs::write("names.index.json", names_index).await?;
    tokio::fs::write("nrdbid.index.json", nrdbid_index).await?;

    Ok(())
}
