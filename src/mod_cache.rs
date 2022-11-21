use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use crate::md5_digest::Md5Digest;

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

    pub fn update_mod_checksum(&mut self, old_checksum: &Md5Digest, new_checksum: Md5Digest) {
        self.mods.remove(old_checksum);
        self.mods.insert(new_checksum);
    }
}
