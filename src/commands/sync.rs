use crate::commands::gen_srf::{gen_srf_for_mod, open_cache_or_gen_srf};
use crate::mod_cache::ModCache;
use crate::{repository, srf};
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use snafu::{ResultExt, Snafu};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor, Read, Seek, SeekFrom};
use std::path::Path;
use tempfile::tempfile;

#[derive(Debug)]
struct DownloadCommand {
    file: String,

    // These are currently unused. TODO: implement file diffing.
    #[allow(dead_code)]
    begin: u64,
    #[allow(dead_code)]
    end: u64,
}

#[derive(Snafu, Debug)]
pub enum Error {
    #[snafu(display("io error: {}", source))]
    Io { source: std::io::Error },
    #[snafu(display("Error while requesting repository data: {}", source))]
    Http {
        url: String,

        #[snafu(source(from(ureq::Error, Box::new)))]
        source: Box<ureq::Error>,
    },
    #[snafu(display("Failed to fetch repository info: {}", source))]
    RepositoryFetch { source: repository::Error },
    #[snafu(display("SRF deserialization failure: {}", source))]
    SrfDeserialization { source: serde_json::Error },
    #[snafu(display("Legacy SRF deserialization failure: {}", source))]
    LegacySrfDeserialization { source: srf::Error },
    #[snafu(display("Failed to generate SRF: {}", source))]
    SrfGeneration { source: srf::Error },
    #[snafu(display("Failed to open ModCache: {}", source))]
    ModCacheOpen { source: crate::mod_cache::Error },
}

fn diff_repo<'a>(
    mod_cache: &ModCache,
    remote_repo: &'a repository::Repository,
) -> Vec<&'a repository::Mod> {
    let mut downloads = Vec::new();

    // repo checksums use the repo generation timestamp in the checksum calculation, so we can't really
    // generate them for comparison. they aren't that useful anyway

    for r#mod in &remote_repo.required_mods {
        if !mod_cache.mods.contains_key(&r#mod.checksum) {
            downloads.push(r#mod);
        }
    }

    downloads
}

fn diff_mod(
    agent: &ureq::Agent,
    repo_base_path: &str,
    local_base_path: &Path,
    remote_mod: &repository::Mod,
) -> Result<Vec<DownloadCommand>, Error> {
    // HACK HACK: this REALLY should be parsed through streaming rather than through buffering the whole thing
    let remote_srf_url = format!("{}{}/mod.srf", repo_base_path, remote_mod.mod_name);
    let mut remote_srf = agent
        .get(&remote_srf_url)
        .call()
        .context(HttpSnafu {
            url: remote_srf_url,
        })?
        .into_reader();

    let mut buf = String::new();
    let _len = remote_srf.read_to_string(&mut buf).context(IoSnafu)?;

    // yeet utf-8 bom, which is bad, not very useful and not supported by serde
    let bomless = buf.trim_start_matches('\u{feff}');

    let remote_is_legacy = srf::is_legacy_srf(&mut Cursor::new(bomless)).context(IoSnafu)?;

    let remote_srf: srf::Mod = if remote_is_legacy {
        srf::deserialize_legacy_srf(&mut BufReader::new(Cursor::new(bomless)))
            .context(LegacySrfDeserializationSnafu)?
    } else {
        serde_json::from_str(bomless).context(SrfDeserializationSnafu)?
    };

    let local_path = local_base_path.join(Path::new(&format!("{}/", remote_mod.mod_name)));
    let srf_path = local_path.join(Path::new("mod.srf"));

    let local_srf = {
        if local_path.exists() {
            let file = File::open(srf_path);

            match file {
                Ok(file) => {
                    let mut reader = BufReader::new(file);

                    if srf::is_legacy_srf(&mut reader).context(IoSnafu)? {
                        srf::deserialize_legacy_srf(&mut reader)
                            .context(LegacySrfDeserializationSnafu)?
                    } else {
                        serde_json::from_reader(&mut reader).context(SrfDeserializationSnafu)?
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    srf::scan_mod(&local_path).context(SrfGenerationSnafu)?
                }
                Err(e) => return Err(Error::Io { source: e }),
            }
        } else {
            srf::Mod::generate_invalid(&remote_srf)
        }
    };

    if local_srf.checksum == remote_srf.checksum {
        return Ok(vec![]);
    }

    let mut local_files = HashMap::new();

    for file in &local_srf.files {
        local_files.insert(&file.path, file);
    }

    let mut remote_files = HashMap::new();

    for file in &remote_srf.files {
        remote_files.insert(&file.path, file);
    }

    let mut download_list = Vec::new();

    for (path, file) in remote_files.drain() {
        let local_file = local_files.remove(path);

        if let Some(local_file) = local_file {
            if file.checksum != local_file.checksum {
                // TODO: implement file diffing. for now, just download everything

                download_list.push(DownloadCommand {
                    file: format!("{}/{}", remote_srf.name, path),
                    begin: 0,
                    end: file.length,
                });
            }
        } else {
            download_list.push(DownloadCommand {
                file: format!("{}/{}", remote_srf.name, path),
                begin: 0,
                end: file.length,
            });
        }
    }

    // remove any local files that remain here
    remove_leftover_files(local_base_path, &remote_srf, local_files.into_values())
        .context(IoSnafu)?;

    Ok(download_list)
}

// remove files that are present in the local disk but not in the remote repo
fn remove_leftover_files<'a>(
    local_base_path: &Path,
    r#mod: &srf::Mod,
    files: impl Iterator<Item = &'a srf::File>,
) -> Result<(), std::io::Error> {
    for file in files {
        let path = file
            .path
            .to_path(local_base_path.join(Path::new(&r#mod.name)));

        println!("removing leftover file {}", &path.display());

        std::fs::remove_file(&path)?;
    }

    Ok(())
}

fn execute_command_list(
    agent: &mut ureq::Agent,
    remote_base: &str,
    local_base: &Path,
    commands: &[DownloadCommand],
) -> Result<(), Error> {
    for (i, command) in commands.iter().enumerate() {
        println!("downloading {} of {} - {}", i, commands.len(), command.file);

        // download into temp file first in case we have a failure. this avoids us writing garbage data
        // which will later make us crash in gen_srf
        let mut temp_download_file = tempfile().context(IoSnafu)?;

        let remote_url = format!("{}{}", remote_base, command.file);

        let response = agent.get(&remote_url).call().context(HttpSnafu {
            url: remote_url.clone(),
        })?;

        let pb = response
            .header("Content-Length")
            .and_then(|len| len.parse().ok())
            .map_or_else(ProgressBar::new_spinner, ProgressBar::new);

        pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .with_key("eta", |state: &ProgressState, w: &mut dyn std::fmt::Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
            .progress_chars("#>-"));

        let reader = response.into_reader();

        std::io::copy(&mut pb.wrap_read(reader), &mut temp_download_file).context(IoSnafu)?;

        // copy from temp to permanent file
        let file_path = local_base.join(Path::new(&command.file));
        std::fs::create_dir_all(file_path.parent().expect("file_path did not have a parent"))
            .context(IoSnafu)?;
        let mut local_file = File::create(&file_path).context(IoSnafu)?;

        temp_download_file
            .seek(SeekFrom::Start(0))
            .context(IoSnafu)?;
        std::io::copy(&mut temp_download_file, &mut local_file).context(IoSnafu)?;
    }

    Ok(())
}

pub fn sync(
    agent: &mut ureq::Agent,
    repo_url: &str,
    base_path: &Path,
    dry_run: bool,
) -> Result<(), Error> {
    let remote_repo = repository::get_repository_info(agent, &format!("{repo_url}/repo.json"))
        .context(RepositoryFetchSnafu)?;

    let mut mod_cache = open_cache_or_gen_srf(base_path).context(ModCacheOpenSnafu)?;

    let check = diff_repo(&mod_cache, &remote_repo);

    println!("mods to check: {check:#?}");

    // remove all mods to check from cache, we'll read them later
    for r#mod in &check {
        mod_cache.remove(&r#mod.checksum);
    }

    let mut download_commands = vec![];

    for r#mod in &check {
        download_commands.extend(diff_mod(agent, repo_url, base_path, r#mod).unwrap());
    }

    println!("download commands: {download_commands:#?}");

    if dry_run {
        return Ok(());
    }

    let res = execute_command_list(agent, repo_url, base_path, &download_commands);

    if let Err(e) = res {
        println!("an error occured while downloading: {e}");
        println!("you should retry this command");
    }

    // gen_srf for the mods we downloaded
    for r#mod in &check {
        let srf = gen_srf_for_mod(&base_path.join(Path::new(&r#mod.mod_name)));

        mod_cache.insert(srf);
    }

    // reserialize the cache
    let writer = BufWriter::new(File::create(base_path.join("nimble-cache.json")).unwrap());
    serde_json::to_writer(writer, &mod_cache).unwrap();

    Ok(())
}
