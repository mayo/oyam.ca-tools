use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod drivers;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub path: std::path::PathBuf,
    pub size: u64,
    pub last_modified: f64,
    pub checksum: Option<String>,
    pub etag: Option<String>,
    pub content_type: Option<String>
}

type ManifestFiles = HashMap<std::string::String, FileMetadata>;


#[derive(Debug, Serialize, Deserialize)]
pub struct SyncManifest {
    pub files: ManifestFiles,
    pub ignore_patterns: Vec<String>
}

impl SyncManifest {
    pub fn new() -> Self {
        SyncManifest {
            files: ManifestFiles::new(),
            ignore_patterns: Vec::new()
        }
    }

    pub fn from_reader<R: std::io::Read>(reader: R) -> Result<Self, Box<dyn std::error::Error>> {
        let manifest: SyncManifest = serde_json::from_reader(reader)?;

        Ok(manifest)
    }

    pub fn to_string(&self) -> Result<String, Box<dyn std::error::Error>> {
        let result = serde_json::to_string(self)?;

        Ok(result)
    }

    pub fn to_vec(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let result = serde_json::to_vec(self)?;

        Ok(result)
    }
}