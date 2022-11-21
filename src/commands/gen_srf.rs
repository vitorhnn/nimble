use crate::mod_cache::ModCache;
use crate::srf;
use rayon::prelude::*;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use walkdir::WalkDir;
use crate::md5_digest::Md5Digest;

pub fn gen_srf_for_mod(mod_path: &Path) -> Md5Digest {
    let generated_srf = srf::scan_mod(mod_path).unwrap();

    let path = mod_path.join("mod.srf");

    let writer = BufWriter::new(File::create(path).unwrap());
    serde_json::to_writer(writer, &generated_srf).unwrap();

    generated_srf.checksum
}

pub fn gen_srf(base_path: &Path) {
    let mods: HashSet<Md5Digest> = WalkDir::new(base_path)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .par_bridge()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir() && e.file_name().to_string_lossy().starts_with('@'))
        .map(|entry| {
            let path = entry.path();
            gen_srf_for_mod(path)
        })
        .collect();

    let cache = ModCache::new(mods);

    let writer = BufWriter::new(File::create(base_path.join("nimble-cache.json")).unwrap());
    serde_json::to_writer(writer, &cache).unwrap();
}
