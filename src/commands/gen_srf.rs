use crate::srf;
use rayon::prelude::*;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use walkdir::WalkDir;

pub fn gen_srf(base_path: &Path) {
    let paths: Vec<_> = WalkDir::new(base_path)
        .max_depth(1)
        .into_iter()
        .skip(1)
        .filter_map(|e| e.ok())
        .map(|entry| entry.path().to_owned())
        .collect();

    let _mods: Vec<_> = paths
        .par_iter()
        .map(|path| {
            let generated_srf = srf::scan_mod(path).unwrap();

            let path = path.join("mod.srf");

            let writer = BufWriter::new(File::create(path).unwrap());
            serde_json::to_writer_pretty(writer, &generated_srf).unwrap();

            generated_srf
        })
        .collect();
}
