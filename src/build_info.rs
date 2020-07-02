use crate::paths::base_dir;
use csv::ReaderBuilder;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub(crate) struct BuildInfo {
    #[serde(default, rename = "Version!STRING:0")]
    pub(crate) version: String,
}

impl BuildInfo {
    pub fn new() -> Self {
        Self {
            version: "".to_string(),
        }
    }
    pub fn get_build_info_from_file() -> BuildInfo {
        let dir = base_dir();
        let build_info_path = dir.join(".build.info");
        let mut rdr = ReaderBuilder::new()
            .delimiter(b'|')
            .from_path(build_info_path)
            .expect("Could not find file");

        for result in rdr.deserialize() {
            if let Ok(x) = result {
                return x;
            }
        }
        BuildInfo::new()
    }
}
