use crate::md5_digest::Md5Digest;
use crate::mod_cache::ModCache;
use crate::srf;
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

pub fn gen_srf(base_path: &Path) {
    let mods: HashMap<Md5Digest, srf::Mod> = WalkDir::new(base_path)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .par_bridge()
        .filter_map(|e| e.ok())
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
