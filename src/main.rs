use snafu::{ResultExt, Whatever};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

mod pbo;
mod repository;
mod srf;
mod commands;


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

fn main() {
    let args = Args::parse();

    let mut agent = ureq::AgentBuilder::new()
        .user_agent("nimble (like Swifty)/0.1")
        .build();

    match args.command {
        Commands::Sync {
            repo_url,
            local_path,
        } => commands::sync::sync(&mut agent, &repo_url, &local_path),
    }
}
