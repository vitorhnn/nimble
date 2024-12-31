use crate::md5_digest::Md5Digest;
use crate::mod_cache::ModCache;
use crate::{mod_cache, srf};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use walkdir::WalkDir;

pub fn gen_srf_for_mod(mod_path: &Path) -> srf::Mod {
    let generated_srf = srf::scan_mod(mod_path).unwrap();

    let path = mod_path.join("mod.srf");

    let writer = BufWriter::new(File::create(path).unwrap());
    serde_json::to_writer(writer, &generated_srf).unwrap();

    generated_srf
}

pub fn open_cache_or_gen_srf(base_path: &Path) -> Result<ModCache, mod_cache::Error> {
    match ModCache::from_disk(base_path) {
        Ok(cache) => Ok(cache),
        Err(mod_cache::Error::FileOpen { source })
            if source.kind() == std::io::ErrorKind::NotFound =>
        {
            println!("nimble-cache.json not found, generating...");
            gen_srf(base_path);
            ModCache::from_disk_or_empty(base_path)
        }
        Err(e) => Err(e),
    }
}

pub fn gen_srf(base_path: &Path) {
    let mods: HashMap<Md5Digest, srf::Mod> = WalkDir::new(base_path)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .par_bridge()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_dir() && e.file_name().to_string_lossy().starts_with('@'))
        .map(|entry| {
            let path = entry.path();
            let srf = gen_srf_for_mod(path);

            (srf.checksum.clone(), srf)
        })
        .collect();

    let cache = ModCache::new(mods);

    cache.to_disk(base_path).unwrap();
}
