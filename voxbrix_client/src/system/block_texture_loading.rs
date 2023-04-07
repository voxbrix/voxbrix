use anyhow::Error;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
};

const PATH: &str = "assets/client/textures/blocks";
const LIST_FILE_NAME: &str = "list.ron";

pub struct BlockTextureLoadingSystem {
    pub size: usize,
    pub textures: Vec<Vec<u8>>,
    pub label_map: BTreeMap<String, u32>,
}

impl BlockTextureLoadingSystem {
    pub async fn load_data() -> Result<Self, Error> {
        blocking::unblock(|| {
            let texture_list = {
                let path = Path::new(PATH).join(LIST_FILE_NAME);
                let string = fs::read_to_string(path)?;
                ron::from_str::<BlockTextureList>(&string)?
            };

            let textures = texture_list
                .list
                .iter()
                .map(|block_texture_label| {
                    let file_name = format!("{}.png", block_texture_label);
                    let path = Path::new(PATH).join(file_name);
                    fs::read(path)
                })
                .collect::<Result<Vec<_>, _>>()?;

            let label_map = texture_list
                .list
                .into_iter()
                .enumerate()
                .map(|(i, t)| (t, i as u32))
                .collect();

            Ok(Self {
                size: texture_list.size,
                textures,
                label_map,
            })
        })
        .await
    }
}

#[derive(Deserialize, Debug)]
struct BlockTextureList {
    size: usize,
    list: Vec<String>,
}
