use crate::commands::gen_srf::open_cache_or_gen_srf;
use crate::mod_cache;
use crate::mod_cache::ModCache;
use snafu::{ResultExt, Snafu};
use std::cfg;
use std::path::{Path, PathBuf};

#[cfg(not(windows))]
use snafu::OptionExt;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("failed to open ModCache: {}", source))]
    ModCacheOpen { source: mod_cache::Error },
    #[snafu(display("failed to find drive_c"))]
    #[cfg(not(windows))]
    FailedToFindDriveC,
}

fn generate_mod_args(base_path: &Path, mod_cache: &ModCache) -> String {
    mod_cache
        .mods
        .values()
        .fold(String::from("-noLauncher -mod="), |acc, r#mod| {
            let mod_name = &r#mod.name;
            let full_path = base_path
                .join(Path::new(mod_name))
                .to_string_lossy()
                .to_string();
            format!("{acc}{full_path};")
        })
}

// if we're on windows we don't have to do anything
#[cfg(windows)]
fn convert_host_base_path_to_proton_base_path(host_base_path: &Path) -> Result<PathBuf, Error> {
    Ok(host_base_path.to_owned())
}

// if we're not on windows, try to find a "drive_c" dir in the ancestors of base_path
#[cfg(not(windows))]
fn convert_host_base_path_to_proton_base_path(host_base_path: &Path) -> Result<PathBuf, Error> {
    let drive_c_path = host_base_path
        .ancestors()
        .find(|&x| x.ends_with("drive_c"))
        .context(FailedToFindDriveCSnafu)?;

    let relative = host_base_path
        .strip_prefix(drive_c_path)
        .expect("drive_c_path was not a prefix of host_base_path, this should never happen");

    Ok(Path::new("c:/").join(relative))
}

pub fn launch(base_path: &Path) -> Result<(), Error> {
    let mod_cache = open_cache_or_gen_srf(base_path).context(ModCacheOpenSnafu)?;

    let proton_base_path = convert_host_base_path_to_proton_base_path(base_path)?;

    let binding = generate_mod_args(&proton_base_path, &mod_cache);
    let cmdline =
        percent_encoding::utf8_percent_encode(&binding, percent_encoding::NON_ALPHANUMERIC);

    let steam_url = format!("steam://run/107410//{cmdline}/");

    dbg!(&steam_url);

    open::that(steam_url).unwrap();

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
        let converted = convert_host_base_path_to_proton_base_path(&original_path).unwrap();

        assert_eq!(original_path, converted);
    }

    #[test]
    #[cfg(not(windows))]
    fn test_proton_path_conversion() {
        // on windows, this should do nothing
        let original_path = PathBuf::from("/home/random/paths/drive_c/banana_repo");
        let converted = convert_host_base_path_to_proton_base_path(&original_path).unwrap();

        assert_eq!(converted, PathBuf::from("c:/banana_repo"));
    }
}
