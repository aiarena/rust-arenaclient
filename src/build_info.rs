use crate::paths::base_dir;
use csv::ReaderBuilder;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub(crate) struct BuildInfo {
    #[serde(default, rename = "Version!STRING:0")]
    pub(crate) version: String,
    #[serde(skip_deserializing )]
    pub(crate) base_build: u32,
    #[serde(skip_deserializing )]
    pub(crate) data_build: u32

}

impl BuildInfo {
    pub fn new() -> Self {
        Self {
            version: "".to_string(),
            base_build: 0,
            data_build: 0
        }
    }
    pub fn get_build_info_from_file() -> BuildInfo {
        let dir = base_dir();
        let build_info_path = dir.join(".build.info");
        let mut rdr = ReaderBuilder::new()
            .delimiter(b'|')
            .from_path(build_info_path)
            .expect("Could not find .build-info file");

        let mut build_info;
        for result in rdr.deserialize::<BuildInfo>() {
            if let Ok(x) = result {
                build_info = x;
                build_info.base_build = build_info.version
                    .split('.')
                    .last()
                    .unwrap()
                    .parse::<u32>().unwrap();
                build_info.data_build = build_info.base_build;
                return build_info;
            }
        }
        BuildInfo::new()
    }
}
