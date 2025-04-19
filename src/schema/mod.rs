use crate::Result;
use awc::Client;
use merge::Merge;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env};
use tracing::error;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename = "items_game")]
pub struct ItemsGameFile {
    pub game_info: GameInfo,
    pub qualities: HashMap<String, Value>,
    pub colors: HashMap<String, Color>,
    pub rarities: HashMap<String, Rarity>,
    pub equip_regions_list: EquipRegionsList,
    pub equip_conflicts: HashMap<String, HashMap<String, u32>>,
    pub quest_objective_conditions: HashMap<String, QuestObjectiveCondition>,
    pub prefabs: HashMap<String, ItemRaw>,
    pub items: HashMap<String, ItemRaw>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Value {
    pub value: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename = "game_info")]
pub struct GameInfo {
    pub first_valid_class: u32,
    pub last_valid_class: u32,
    pub account_class_index: u32,
    pub account_first_valid_item_slot: u32,
    pub account_last_valid_item_slot: u32,
    pub first_valid_item_slot: u32,
    pub last_valid_item_slot: u32,
    pub num_item_presets: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub color_name: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rarity {
    pub value: u32,
    pub loc_key: String,
    pub loc_key_weapon: String,
    pub color: String,
    pub drop_sound: Option<String>,
    pub next_rarity: Option<String>,
    pub loot_list: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EquipRegionsList {
    pub whole_head: String,
    pub hat: String,
    pub face: String,
    pub glasses: String,
    pub lenses: String,
    pub shared: Vec<HashMap<String, String>>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestObjectiveCondition {
    pub name: String,
    pub condition_logic: ConditionLogic,
    pub required_items: Option<HashMap<String, RequiredItem>>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename = "condition_logic")]
pub struct ConditionLogic {
    #[serde(rename = "type")]
    pub type_field: Option<String>,
    pub event_name: Option<String>,
    #[serde(flatten)]
    pub other: HashMap<String, serde_json::Value>, // Use serde_json::Value to handle dynamic content
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequiredItem {
    pub loaner_defindex: String,
    pub qualifying_items: HashMap<String, QualifyingItem>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualifyingItem {
    pub defindex: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Prefab {
    pub craft_class: Option<String>,
    pub attributes: Option<HashMap<String, Attribute>>,
    pub capabilities: Option<HashMap<String, String>>,
    pub holiday_restriction: Option<String>,
    pub show_in_armory: Option<String>,
    pub item_class: Option<String>,
    pub image_inventory: Option<String>,
    pub min_ilevel: Option<u32>,
    pub max_ilevel: Option<u32>,
    pub public_prefab: Option<String>,
    pub tags: Option<HashMap<String, String>>,
    #[serde(default)]
    pub prefab: Vec<String>, // For nested prefabs
    pub static_attrs: Option<HashMap<String, String>>,
    pub item_type_name: Option<String>,
    pub item_slot: Option<String>,
    pub item_quality: Option<String>,
    pub visuals: Option<Visuals>,
    pub equip_region: Option<String>,
    pub used_by_classes: Option<HashMap<String, String>>,
    pub tool: Option<Tool>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Attribute {
    Float(FloatAttribute),
    String(StringAttribute),
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FloatAttribute {
    pub attribute_class: String,
    pub value: f32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StringAttribute {
    pub attribute_class: String,
    pub value: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Visuals {
    #[serde(rename = "player_bodygroups")]
    pub player_bodygroups: Option<HashMap<String, String>>,
    pub styles: Option<HashMap<String, Style>>,
    #[serde(rename = "animation_replacement")]
    pub animation_replacement: Option<HashMap<String, String>>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub skin: Option<u32>,
    pub name: Option<String>,
    pub skin_red: Option<u32>,
    pub skin_blu: Option<u32>,
    pub model_player_per_class: Option<HashMap<String, String>>,
    pub additional_hidden_bodygroups: Option<HashMap<String, String>>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub type_field: Option<String>,
    pub usage_capabilities: Option<HashMap<String, String>>,
    pub usage: Option<Usage>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub item_desc_tool_target: Option<String>,
    pub required_tags: Option<HashMap<String, String>>,
    pub attributes: Option<HashMap<String, String>>,
}

fn overwrite<T>(left: &mut Option<T>, right: Option<T>) {
    if right.is_some() {
        *left = right;
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, merge::Merge)]
pub struct ItemRaw {
    #[merge(strategy = overwrite)]
    pub name: Option<String>, // Present for prefabs, not others
    #[merge(strategy = overwrite)]
    pub hidden: Option<String>,
    #[merge(strategy = overwrite)]
    pub item_class: Option<String>,
    #[merge(strategy = overwrite)]
    pub item_name: Option<String>,

    // Only Vec since some items are written with multiple type names :(
    #[serde(default)]
    #[merge(strategy = merge::vec::append)]
    pub item_type_name: Vec<String>,
    #[merge(strategy = overwrite)]
    pub item_slot: Option<String>,
    #[merge(strategy = overwrite)]
    pub item_quality: Option<String>,
    #[merge(strategy = overwrite)]
    pub min_ilevel: Option<u32>,
    #[merge(strategy = overwrite)]
    pub max_ilevel: Option<u32>,
    #[serde(default)]
    #[merge(strategy = merge::vec::append)]
    pub prefab: Vec<String>,
    #[merge(strategy = overwrite)]
    pub baseitem: Option<String>,
    #[merge(strategy = overwrite)]
    pub item_logname: Option<String>,

    // Only Vec since some items are written with multiple attribute blocks :(
    #[serde(default)]
    #[merge(strategy = merge::vec::append)]
    pub attributes: Vec<HashMap<String, Attribute>>,
}

impl ItemRaw {
    pub fn into_item(self) -> Item {
        Item {
            name: self.name,
            hidden: self.hidden,
            item_class: self.item_class,
            item_name: self.item_name,
            item_type_name: self.item_type_name,
            item_slot: self.item_slot,
            item_quality: self.item_quality,
            min_ilevel: self.min_ilevel,
            max_ilevel: self.max_ilevel,
            prefab: self.prefab,
            baseitem: self.baseitem,
            item_logname: self.item_logname,
            attributes: self
                .attributes
                .into_iter()
                .fold(HashMap::default(), |mut a, b| {
                    a.extend(b);
                    a
                }),
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq)]
pub struct Item {
    pub name: Option<String>, // Present for prefabs, not others
    pub hidden: Option<String>,
    pub item_class: Option<String>,
    pub item_name: Option<String>,

    pub item_type_name: Vec<String>,
    pub item_slot: Option<String>,
    pub item_quality: Option<String>,
    pub min_ilevel: Option<u32>,
    pub max_ilevel: Option<u32>,
    pub prefab: Vec<String>,
    pub baseitem: Option<String>,
    pub item_logname: Option<String>,

    pub attributes: HashMap<String, Attribute>,
}

#[derive(Default, Debug, Clone)]
pub struct Schema {
    pub items: HashMap<u32, Item>,
    pub prefabs: HashMap<String, ItemRaw>,
}

impl Schema {
    pub fn make_prefab(&self, s: &str) -> ItemRaw {
        let mut out = ItemRaw::default();
        if let Some(op) = self.prefabs.get(s) {
            for p in op.prefab.iter().flat_map(|s| s.split(" ")) {
                let pi = self.make_prefab(p);
                out.merge(pi.clone());
            }
            out.merge(op.clone());
        } else {
            error!("Unknown prefab: {s}");
        }
        out
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SchemaUrl {
    status: i32,
    items_game_url: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ApiResponse<T> {
    result: T,
}

async fn fetch_bytes() -> Result<String> {
    let schema_path_var = std::env::var("TF2_SCHEMA_PATH").or(env::var("DEMO_TF2_SCHEMA_PATH"));
    if let Ok(schema_string) = schema_path_var {
        let schema_path = std::path::Path::new(&schema_string);
        return Ok(std::fs::read_to_string(schema_path)
            .map_err(|e| format!("Error {e}: While reading {schema_path:?}"))?);
    }

    let api_key = env::var("STEAM_API_KEY")
        .or(env::var("DEMO_STEAM_API_KEY"))
        .expect("STEAM_API_KEY must be set")
        .to_string();
    let client = Client::default();

    let schema_url =
        format!("https://api.steampowered.com/IEconItems_440/GetSchemaURL/v0001/?key={api_key}");
    let mut response = client.get(schema_url).send().await?;
    let body: ApiResponse<SchemaUrl> = response.json().await?;

    let vdf_url = body.result.items_game_url;
    let mut response = client.get(vdf_url).send().await?;
    Ok(std::str::from_utf8(&response.body().await?)?.to_string())
}

pub async fn fetch() -> Result<Schema> {
    let s = fetch_bytes().await?;
    let v = keyvalues_serde::from_str_raw::<ItemsGameFile>(&s)?;

    let mut schema = Schema {
        items: Default::default(),
        prefabs: v.prefabs,
    };

    let def = v
        .items
        .get("default")
        .ok_or("Schema is missing default item entry")?
        .clone();

    for (k, i) in v.items {
        if k == "default" {
            continue;
        }

        let mut new_i = def.clone();
        for p in i.prefab.iter().flat_map(|s| s.split(" ")) {
            new_i.merge(schema.make_prefab(p));
        }
        new_i.merge(i);
        schema.items.insert(k.parse::<u32>()?, new_i.into_item());
    }

    Ok(schema)
}
