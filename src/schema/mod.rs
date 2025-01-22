use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Deserializer;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub name: String,
    pub defindex: u32,
    pub item_class: String,
    pub item_type_name: String,
    pub item_name: String,
    pub item_description: Option<String>,
    pub proper_name: bool,
    pub item_slot: Option<String>,
    pub model_player: Option<String>,
    pub item_quality: i64,
    pub image_inventory: Option<String>,
    pub min_ilevel: i64,
    pub max_ilevel: i64,
    pub image_url: Option<String>,
    pub image_url_large: Option<String>,
    pub drop_type: Option<String>,
    pub craft_class: Option<String>,
    pub craft_material_type: Option<String>,
    pub capabilities: Capabilities,
    #[serde(default)]
    pub used_by_classes: Vec<String>,
    #[serde(default)]
    pub attributes: Vec<Attribute>,
    pub item_set: Option<String>,
    pub tool: Option<Tool>,
    pub holiday_restriction: Option<String>,
    #[serde(default)]
    pub styles: Vec<Style>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    #[serde(default)]
    pub nameable: bool,
    #[serde(default)]
    pub can_craft_if_purchased: bool,
    #[serde(default)]
    pub can_gift_wrap: bool,
    #[serde(default)]
    pub can_craft_count: bool,
    #[serde(default)]
    pub can_craft_mark: bool,
    #[serde(default)]
    pub can_be_restored: bool,
    #[serde(default)]
    pub strange_parts: bool,
    #[serde(default)]
    pub can_card_upgrade: bool,
    #[serde(default)]
    pub can_strangify: bool,
    #[serde(default)]
    pub can_killstreakify: bool,
    #[serde(default)]
    pub can_consume: bool,
    #[serde(default)]
    pub paintable: bool,
    #[serde(default)]
    pub usable_gc: bool,
    #[serde(default)]
    pub usable_out_of_game: bool,
    #[serde(default)]
    pub can_unusualify: bool,
    #[serde(default)]
    pub decodable: bool,
    #[serde(default)]
    pub usable: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attribute {
    pub name: String,
    pub class: String,
    pub value: f64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub type_field: String,
    pub usage_capabilities: Option<UsageCapabilities>,
    pub restriction: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageCapabilities {
    pub can_killstreakify: Option<bool>,
    pub strange_parts: Option<bool>,
    pub decodable: Option<bool>,
    pub can_card_upgrade: Option<bool>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub name: String,
    pub additional_hidden_bodygroups: Option<AdditionalHiddenBodygroups>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdditionalHiddenBodygroups {
    pub headphones: Option<i64>,
    pub hat: Option<i64>,
    pub grenades: Option<i64>,
}

pub type Schema = HashMap<u32, Item>;

pub fn parse(path: &std::path::Path) -> std::io::Result<Schema> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);

    let mut map = HashMap::with_capacity(10_000); // A bit larger than the scheam in Jan 2025
    for item in Deserializer::from_reader(reader).into_iter::<Item>() {
        match item {
            Ok(item) => {
                let defidx = item.defindex;
                map.insert(defidx, item);
            }
            Err(e) => tracing::error!("{e}"),
        }
    }

    Ok(map)
}
