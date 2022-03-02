use crate::repository::replicate_remote_repo_info;
use snafu::{ResultExt, Whatever};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

mod pbo;
mod repository;
mod srf;

fn diff_repos<'a>(
    local_repo: &repository::Repository,
    remote_repo: &'a repository::Repository,
) -> Vec<&'a repository::Mod> {
    let mut downloads = Vec::new();

    if local_repo.checksum == remote_repo.checksum {
        return vec![];
    }

    let mut checksum_map = HashMap::new();

    for _mod in &local_repo.required_mods {
        checksum_map.insert(&_mod.checksum, _mod);
    }

    for _mod in &remote_repo.required_mods {
        match checksum_map.get(&_mod.checksum) {
            None => downloads.push(_mod),
            Some(local_mod) if local_mod.checksum != _mod.checksum => downloads.push(_mod),
            _ => (),
        }
    }

    downloads
}

#[derive(Debug)]
struct DownloadCommand {
    file: String,
    begin: u64,
    end: u64,
}

fn diff_mod(
    agent: &ureq::Agent,
    repo_base_path: &str,
    local_base_path: &Path,
    remote_mod: &repository::Mod,
) -> Result<Vec<DownloadCommand>, Whatever> {
    // HACK HACK: this REALLY should be parsed through streaming rather than through buffering the whole thing
    let mut remote_srf = agent
        .get(&format!(
            "{}{}/mod.srf",
            repo_base_path, remote_mod.mod_name
        ))
        .call()
        .unwrap()
        .into_reader();

    let mut buf = String::new();
    let _len = remote_srf.read_to_string(&mut buf).unwrap();

    // yeet utf-8 bom, which is bad, not very useful and not supported by serde
    let bomless = buf.trim_start_matches("\u{feff}");

    let remote_srf: srf::Mod = serde_json::from_str(&bomless).unwrap(); /*.or_else(|_| {
                                                                            srf::deserialize_legacy_srf(&mut BufReader::new(Cursor::new(remote_srf)))
                                                                        }).with_whatever_context(|_| "failed to deserialize remote srf")?;*/

    let local_path = local_base_path.join(Path::new(&format!("{}/", remote_mod.mod_name)));
    let srf_path = local_path.join(Path::new("mod.srf"));

    let local_srf = {
        if !local_path.exists() {
            srf::Mod::generate_invalid(&remote_srf)
        } else {
            let file = File::open(&srf_path);

            match file {
                Ok(file) => {
                    let mut reader = BufReader::new(file);

                    serde_json::from_reader(&mut reader)
                        .or_else(|_| srf::deserialize_legacy_srf(&mut reader))
                        .with_whatever_context(|_| "failed to deserialize local srf")?
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    srf::scan_mod(&local_path).unwrap()
                }
                _ => panic!(),
            }
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
                    file: format!("{}/{}", remote_srf.name, path.to_string().to_string()),
                    begin: 0,
                    end: file.length,
                })
            }
        } else {
            download_list.push(DownloadCommand {
                file: format!("{}/{}", remote_srf.name, path.to_string().to_string()),
                begin: 0,
                end: file.length,
            })
        }
    }

    Ok(download_list)
}

fn execute_command_list(
    agent: &mut ureq::Agent,
    remote_base: &str,
    local_base: &Path,
    commands: &[DownloadCommand],
) {
    for (i, command) in commands.iter().enumerate() {
        println!("downloading {} of {}", i, commands.len());

        let file_path = local_base.join(Path::new(&command.file));
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        let mut local_file = File::create(&file_path).unwrap();

        let remote_url = format!("{}{}", remote_base, command.file);

        let mut reader = agent.get(&remote_url).call().unwrap().into_reader();

        std::io::copy(&mut reader, &mut local_file).unwrap();
    }
}

#[derive(Subcommand)]
enum Commands {
    Sync {
        #[clap(short, long)]
        repo_url: String,

        #[clap(short, long)]
        local_path: PathBuf,
    },
}

#[derive(Parser)]
struct Args {
    #[clap(subcommand)]
    command: Commands,
}

fn sync(agent: &mut ureq::Agent, repo_url: &str, base_path: &Path) {
    let remote_repo =
        repository::get_repository_info(agent, &format!("{}/repo.json", repo_url)).unwrap();

    let local_repo: repository::Repository = {
        let file = std::fs::File::open(base_path.join("./repo.json"));

        match file {
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    replicate_remote_repo_info(&remote_repo)
                } else {
                    panic!();
                }
            }
            Ok(file) => serde_json::from_reader(BufReader::new(file)).unwrap(),
        }
    };

    let check = diff_repos(&local_repo, &remote_repo);

    println!("mods to check: {:#?}", check);

    let mut download_commands = vec![];

    for _mod in check {
        download_commands.extend(diff_mod(agent, repo_url, base_path, _mod).unwrap());
    }

    println!("download commands: {:#?}", download_commands);

    execute_command_list(agent, repo_url, base_path, &download_commands);
}

fn main() {
    let args = Args::parse();

    let mut agent = ureq::AgentBuilder::new()
        .user_agent("nimble (like Swifty)/0.1")
        .build();

    match args.command {
        Commands::Sync {
            repo_url,
            local_path,
        } => sync(&mut agent, &repo_url, &local_path),
    }
}
