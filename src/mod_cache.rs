use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use snafu::{Snafu, ResultExt};
use crate::md5_digest::Md5Digest;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("failed to create cache file: {}", source))]
    FileCreation { source: std::io::Error },
    #[snafu(display("failed to open cache file: {}", source))]
    FileOpen { source: std::io::Error },
    #[snafu(display("serde failed to serialize: {}", source))]
    Serialization { source: serde_json::Error },
    #[snafu(display("serde failed to deserialize: {}", source))]
    Deserialization { source: serde_json::Error },
}

#[derive(Serialize, Deserialize)]
pub struct ModCache {
    version: u32,
    pub mods: HashSet<Md5Digest>,
}

impl ModCache {
    pub fn new(mods: HashSet<Md5Digest>) -> Self {
        Self { version: 1, mods }
    }

    pub fn new_empty() -> Self {
        Self {
            version: 1,
            mods: HashSet::new(),
        }
    }

    pub fn from_disk_or_empty(repo_path: &Path) -> Result<Self, Error> {
        let path = repo_path.join("nimble-cache.json");
        let open_result = File::open(path);
        match open_result {
            Ok(file) => {
                let reader = BufReader::new(file);
                serde_json::from_reader(reader).context(DeserializationSnafu)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(Self::new_empty())
            },
            Err(e) => Err(Error::FileOpen { source: e })
        }
    }

    pub fn to_disk(&self, repo_path: &Path) -> Result<(), Error> {
        let path = repo_path.join("nimble-cache.json");
        let file = File::create(path).context(FileCreationSnafu)?;
        let writer = BufWriter::new(file);

        serde_json::to_writer(writer, &self).context(SerializationSnafu)?;

        Ok(())
    }

    pub fn update_mod_checksum(&mut self, old_checksum: &Md5Digest, new_checksum: Md5Digest) {
        self.mods.remove(old_checksum);
        self.mods.insert(new_checksum);
    }
}
