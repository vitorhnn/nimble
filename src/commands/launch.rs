use crate::mod_cache;
use crate::mod_cache::ModCache;
use snafu::{ResultExt, Snafu};
use std::path::{PathBuf, Path};
use std::cfg;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("failed to open ModCache: {}", source))]
    ModCacheOpen { source: mod_cache::Error },
}

fn generate_mod_args(base_path: &Path, mod_cache: &ModCache) -> String {
    mod_cache.mods.values().fold(String::from("-mod="), |acc, r#mod| {
        let mod_name = &r#mod.name;
        let full_path = base_path.join(Path::new(mod_name)).to_string_lossy().to_string();
        format!("{acc}{full_path};")
    })
}

// if we're on windows we don't have to do anything
#[cfg(windows)]
fn convert_host_base_path_to_proton_base_path(host_base_path: PathBuf) -> PathBuf {
    host_base_path
}

// if we're not on windows, try to find a "drive_c" dir in the ancestors of base_path
#[cfg(not(windows))]
fn convert_host_base_path_to_proton_base_path(host_base_path: PathBuf) -> PathBuf {
    let drive_c_path = host_base_path.ancestors().find(|&x| x.ends_with("drive_c")).unwrap();

    let relative = host_base_path.strip_prefix(drive_c_path).unwrap();

    Path::new("c:/").join(relative)
}

pub fn launch(base_path: PathBuf) -> Result<(), Error> {
    let mod_cache = ModCache::from_disk_or_empty(&base_path).context(ModCacheOpenSnafu)?;

    let proton_base_path = convert_host_base_path_to_proton_base_path(base_path);

    let cmdline = mod_cache.mods.values().fold(String::from("-mod="), |acc, r#mod| {
        let mod_name = &r#mod.name;
        let full_path = proton_base_path.join(Path::new(mod_name)).to_string_lossy().to_string();
        format!("{acc}{full_path};")
    });

    dbg!(cmdline);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn test_proton_path_conversion() {
        // on windows, this should do nothing
        let original_path = PathBuf::from("C:\\random\\paths\\drive_c\\banana_repo");
        let converted = convert_host_base_path_to_proton_base_path(original_path.clone());

        assert_eq!(original_path, converted);
    }

    #[test]
    #[cfg(not(windows))]
    fn test_proton_path_conversion() {
        // on windows, this should do nothing
        let original_path = PathBuf::from("/home/random/paths/drive_c/banana_repo");
        let converted = convert_host_base_path_to_proton_base_path(original_path.clone());

        assert_eq!(converted, PathBuf::from("c:/banana_repo"));
    }
}
