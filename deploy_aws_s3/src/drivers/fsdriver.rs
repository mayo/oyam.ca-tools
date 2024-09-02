use std::fs;
use sha2::{Sha256, Digest};
use walkdir::{DirEntry, WalkDir};
use crate::{FileMetadata, SyncManifest};
use crate::drivers::Driver;

pub struct FSDriver {
    base_directory: std::path::PathBuf
}

impl FSDriver {

    pub fn new(base_directory: std::path::PathBuf) -> FSDriver {
        FSDriver {
            base_directory: base_directory
        }
    }

    //TODO: implement specific ignore file, for eg. the txn-log file.
    //TODO: use ignore patterns from SyncManifest initialization
    fn is_not_ignored_file(entry: &DirEntry) -> bool {
        entry
            .file_name()
            .to_str()
            
            //filter dotfiles
            .map(|s| entry.depth() == 0 || !s.starts_with("."))

            .unwrap_or(false)
    }

    fn local_file_hash(path: &std::path::Path) -> Result<String, Box <dyn std::error::Error>> {
        let mut file = fs::File::open(&path)?;
        let mut hasher = Sha256::new();
        
        std::io::copy(&mut file, &mut hasher)?;
        
        let hash = hasher.finalize();

        return Ok(base16ct::lower::encode_string(&hash));
    }
}

impl Driver for FSDriver {
    async fn build_manifest(&self) -> SyncManifest {
        //Result<FileManifest, Box<dyn Error>> {
        //TODO skip ignore files

        // let cookie = magic::Cookie::open(magic::cookie::Flags::MIME_TYPE).expect("cookie");
        // let database = Default::default();
        // let cookie = cookie.load(&database).expect("loaded magic database");

        let mut manifest = SyncManifest::new();
        
        for entry in WalkDir::new(&self.base_directory)
                .into_iter()
                .filter_entry(|e| FSDriver::is_not_ignored_file(e))
                .filter_map(|e| e.ok()) {
            
            let entry_metadata = entry.metadata().unwrap();
    
            // skipping directories at walk level will not descend into them
            if entry_metadata.is_dir() {
                continue;
            }
    
            let ctype = mime_guess::from_path(entry.path().to_path_buf()).first_or_octet_stream().to_string();

            //TODO: do symlinks need special treatment?
            manifest.files.insert(
                entry.path().strip_prefix(&self.base_directory).expect("The file should not be from outside of base").to_str().expect("UTF8 only").to_string(),
                FileMetadata {
                    path: entry.path().to_path_buf(),
                    size: entry_metadata.len(),
                    last_modified: entry_metadata.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64(),
                    etag: None,
                    checksum: Some(FSDriver::local_file_hash(entry.path()).unwrap()),
                    //content_type: Some(cookie.file(entry.path().to_path_buf()).expect("mime"))
                    content_type: Some(mime_guess::from_path(entry.path().to_path_buf()).first_or_octet_stream().to_string())
                }
            );
        }

        return manifest;

    }
}


    // for entry in WalkDir::new(args.source).into_iter().filter_map(|e| e.ok()) {
    //     println!("{}", entry.path().display());
    // }

