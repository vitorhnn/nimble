use crate::mod_cache;
use crate::mod_cache::ModCache;
use snafu::{OptionExt, ResultExt, Snafu};
use std::cfg;
use std::path::{Path, PathBuf};
use steamlocate::{SteamDir, CompatTool};

const ARMA3_APPID: u32 = 107410;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("failed to open ModCache: {}", source))]
    ModCacheOpen { source: mod_cache::Error },
    #[snafu(display("steamlocate error: {}", source))]
    SteamLocate { source: steamlocate::Error },
    #[snafu(display("failed to find drive_c"))]
    FailedToFindDriveC,
    #[snafu(display("couldn't find arma"))]
    FailedToFindArma,
    #[snafu(display("read dir IO error: {}", source))]
    ReadDir { source: std::io::Error },
    #[snafu(display("couldn't find compat tool"))]
    CompatToolNotFound,
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

pub fn find_compat_tool(steam_dir: &SteamDir, compat_tool: &CompatTool) -> Result<PathBuf, Error> {
    // try to find it in steam compatibilitytools.d, if not found, try to find it in any library
    let steam_path = steam_dir.path();
    let compat_tool_name = compat_tool.name.as_ref().expect("no compat tool name");
    let mut compat_tool_path = steam_path.join("compatibilitytools.d");
    compat_tool_path.push(compat_tool_name);

    if compat_tool_path.exists() {
        return Ok(compat_tool_path);
    }

    // we have to do a weird scan of all steam libraries.
    // a lot of this is adapted from github.com/muttleyxd/arma3-unix-launcher
    if let Some((library, app)) = 'found: {
        for library in steam_dir
            .libraries()
            .context(SteamLocateSnafu)?
            .filter_map(Result::ok) {
            for app in library.apps().filter_map(Result::ok) {
                if app.name.as_ref().is_some_and(|name| name == compat_tool_name) {
                    break 'found Some((library, app));
                }
            }
        }
        None
    } {
        Ok(library.resolve_app_dir(&app))
    } else {
        Err(Error::CompatToolNotFound)
    }
}

pub fn launch_directly(arma_path: &Path) {
    todo!();
}

pub fn launch_compat_tool(arma_path: &Path, compat_tool_path: &Path) {
    todo!();
}

pub fn launch(base_path: PathBuf) -> Result<(), Error> {
    let mod_cache = ModCache::from_disk_or_empty(&base_path).context(ModCacheOpenSnafu)?;

    let proton_base_path = convert_host_base_path_to_proton_base_path(&base_path)?;

    let steam_dir = steamlocate::SteamDir::locate().context(SteamLocateSnafu)?;
    if let Some((arma, library)) = steam_dir.find_app(ARMA3_APPID).context(SteamLocateSnafu)? {
        let arma_path = library.resolve_app_dir(&arma);
        if let Some(compat_tool) = steam_dir.compat_tool_mapping().context(SteamLocateSnafu)?.get(&ARMA3_APPID) {
            let compat_tool_path = find_compat_tool(&steam_dir, &compat_tool)?;
            dbg!("compat tool is {}, path {}", compat_tool, compat_tool_path);
            todo!();
            launch_compat_tool(&arma_path, &compat_tool_path);
        } else {
            launch_directly(&arma_path);
        }

        Ok(())
    } else {
        Err(Error::FailedToFindArma)
    }
    /*
    let binding = generate_mod_args(&proton_base_path, &mod_cache);
    let cmdline =
        percent_encoding::utf8_percent_encode(&binding, percent_encoding::NON_ALPHANUMERIC);

    let steam_url = format!("steam://run/107410//{cmdline}/");

    dbg!(&steam_url);

    open::that(steam_url).unwrap();

    Ok(())
     */
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
