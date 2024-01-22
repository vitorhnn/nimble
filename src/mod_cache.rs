use crate::md5_digest::Md5Digest;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

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

#[derive(Serialize, Deserialize, Debug)]
pub struct Mod {
    pub name: String,
}

impl From<crate::srf::Mod> for Mod {
    fn from(value: crate::srf::Mod) -> Self {
        Mod { name: value.name }
    }
}

type SrfMod = crate::srf::Mod;

#[derive(Serialize, Deserialize)]
pub struct ModCache {
    version: u32,
    pub mods: HashMap<Md5Digest, Mod>,
}

impl ModCache {
    pub fn new(mods: HashMap<Md5Digest, SrfMod>) -> Self {
        Self {
            version: 1,
            mods: mods.into_iter().map(|(k, v)| (k, v.into())).collect(),
        }
    }

    pub fn new_empty() -> Self {
        Self {
            version: 1,
            mods: HashMap::new(),
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
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::new_empty()),
            Err(e) => Err(Error::FileOpen { source: e }),
        }
    }

    pub fn to_disk(&self, repo_path: &Path) -> Result<(), Error> {
        let path = repo_path.join("nimble-cache.json");
        let file = File::create(path).context(FileCreationSnafu)?;
        let writer = BufWriter::new(file);

        serde_json::to_writer(writer, &self).context(SerializationSnafu)?;

        Ok(())
    }

    pub fn remove(&mut self, checksum: &Md5Digest) {
        self.mods.remove(checksum);
    }

    pub fn insert(&mut self, r#mod: crate::srf::Mod) {
        self.mods.insert(r#mod.checksum.clone(), r#mod.into());
    }
}
