//! Labelled screenshot corpus (`labels.json` next to the images).
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize)]
pub struct Label {
    #[serde(default)]
    pub negative: bool,
    #[serde(default)]
    pub matrix: Vec<Vec<u8>>,
    #[serde(default)]
    pub sequences: Vec<Vec<u8>>,
    #[serde(default)]
    pub buffer: usize,
    #[serde(default = "latin")]
    pub style: String,
}

fn latin() -> String {
    "latin".into()
}

pub struct Item {
    pub name: String,
    pub path: PathBuf,
    pub label: Label,
}

pub fn load(dir: &Path) -> Result<Vec<Item>, String> {
    let text = std::fs::read_to_string(dir.join("labels.json"))
        .map_err(|e| format!("labels.json: {e}"))?;
    let map: BTreeMap<String, Label> =
        serde_json::from_str(&text).map_err(|e| format!("labels.json: {e}"))?;
    let mut items: Vec<Item> =
        map.into_iter().map(|(name, label)| Item { path: dir.join(&name), name, label }).collect();
    items.sort_by_key(|i| i.name.trim_end_matches(".png").parse::<u32>().unwrap_or(u32::MAX));
    Ok(items)
}
