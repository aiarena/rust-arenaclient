//! Map file finder

use std::fs;

use crate::paths::map_dir;

/// Find a map file, returning its relative path to the sc2 map directory
pub fn find_map(mut name: String) -> Option<String> {
    name = name.replace(' ', "");
    if !name.ends_with(".SC2Map") {
        name.push_str(".SC2Map");
    }

    let mapdir = map_dir();
    for outer in fs::read_dir(mapdir.clone()).expect("Could not iterate map directory") {
        let outer_path = outer.unwrap().path();
        if !outer_path.is_dir() {
            let current = outer_path
                .file_name()
                .unwrap()
                .to_str()
                .expect("Invalid unicode in path");
            if current.to_ascii_lowercase() == name.to_ascii_lowercase() {
                let relative = outer_path.strip_prefix(mapdir).unwrap();
                let relative_str = relative.to_str().unwrap();
                return Some(relative_str.to_owned());
            } else {
                continue;
            }
        }

        for inner in fs::read_dir(outer_path).expect("Could not iterate map subdirectory") {
            let path = inner.unwrap().path();
            let current = path
                .file_name()
                .unwrap()
                .to_str()
                .expect("Invalid unicode in path");
            if current.to_ascii_lowercase() == name.to_ascii_lowercase() {
                let relative = path.strip_prefix(mapdir).unwrap();
                let relative_str = relative.to_str().unwrap();
                return Some(relative_str.to_owned());
            }
        }
    }
    None
}
