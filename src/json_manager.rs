//! Port of `JsonManager.cs`: a value that is loaded from and saved to one JSON file.

use std::fs;
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::Serialize;

pub struct JsonManager<T> {
    pub prop: T,
    pub class_name: &'static str,
    pub file_location: PathBuf,
}

impl<T: Serialize + DeserializeOwned + Default> JsonManager<T> {
    pub fn new(class_name: &'static str, file_location: PathBuf) -> Self {
        Self { prop: T::default(), class_name, file_location }
    }

    /// Returns false when the file was missing or unreadable (defaults are used then).
    pub fn load(&mut self) -> bool {
        let ident = format!("{}::Load", self.class_name);
        crate::log!(&ident, "Loading from {}", self.file_location.display());

        let text = match fs::read_to_string(&self.file_location) {
            Ok(t) => t,
            Err(_) => {
                crate::log!(&ident, "Failed to read file, using defaults");
                self.prop = T::default();
                return false;
            }
        };
        // tolerate a UTF-8 BOM
        match serde_json::from_str::<T>(text.trim_start_matches('\u{feff}')) {
            Ok(v) => {
                self.prop = v;
                true
            }
            Err(e) => {
                crate::log!(&ident, "Failed to parse: {e}");
                let backup = self.file_location.with_extension("json.bak");
                let _ = fs::copy(&self.file_location, backup);
                self.prop = T::default();
                false
            }
        }
    }

    pub fn save(&self) {
        let ident = format!("{}::Save", self.class_name);
        if let Some(parent) = self.file_location.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(&self.prop) {
            Ok(s) => {
                if let Err(e) = fs::write(&self.file_location, s) {
                    crate::log!(&ident, "Failed to write {}: {e}", self.file_location.display());
                }
            }
            Err(e) => crate::log!(&ident, "Failed to serialise: {e}"),
        }
    }
}
