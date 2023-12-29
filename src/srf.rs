use crate::md5_digest::Md5Digest;
use md5::{Digest, Md5};
use rayon::prelude::*;
use relative_path::RelativePathBuf;
use serde::{Deserialize, Deserializer, Serialize};
use snafu::{OptionExt, ResultExt, Snafu};
use std::ffi::OsStr;
use std::io::{BufReader, Seek, SeekFrom};
use std::{
    io,
    io::{BufRead, Read},
    path::Path,
};
use walkdir::WalkDir;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Part {
    path: String,
    length: u64,
    start: u64,
    checksum: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum FileType {
    #[serde(rename = "SwiftyFile")]
    File,
    #[serde(rename = "SwiftyPboFile")]
    Pbo,
}

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("io error: {}", source))]
    Io { source: io::Error },
    #[snafu(display("pbo error: {}", source))]
    Pbo { source: crate::pbo::Error },
    #[snafu(display("legacy srf parse failure: {}", description))]
    LegacySrfParseFailure { description: &'static str },
    #[snafu(display("legacy srf failed to parse size as u32: {}", source))]
    LegacySrfU32ParseFailure { source: std::num::ParseIntError },
    #[snafu(display("failed to decode md5 digest: {}", source))]
    DigestParse { source: crate::md5_digest::Error },
}

impl FileType {
    fn from_legacy_srf(legacy_type: &str) -> Result<Self, Error> {
        match legacy_type {
            "PBO" => Ok(Self::Pbo),
            "FILE" => Ok(Self::File),
            _ => Err(Error::LegacySrfParseFailure {
                description: "unknown legacy file type",
            }),
        }
    }
}

// needed because swifty doesn't (didn't?) normalize windows paths
pub fn deserialize_relative_pathbuf<'de, D>(deserializer: D) -> Result<RelativePathBuf, D::Error>
where
    D: Deserializer<'de>,
{
    let stringly = String::deserialize(deserializer)?;
    Ok(RelativePathBuf::from_path(stringly.replace('\\', "/")).unwrap())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct File {
    #[serde(deserialize_with = "deserialize_relative_pathbuf")]
    pub path: RelativePathBuf,
    pub length: u64,
    pub checksum: String,
    pub r#type: FileType,
    pub parts: Vec<Part>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Mod {
    pub name: String,
    pub checksum: Md5Digest,
    pub files: Vec<File>,
}

impl Mod {
    pub fn generate_invalid(remote: &Self) -> Self {
        Self {
            checksum: Default::default(),
            files: vec![],
            ..remote.clone()
        }
    }
}

fn generate_hash(file: &mut BufReader<std::fs::File>, len: u64) -> Result<String, Error> {
    let mut hasher = Md5::new();
    let mut stream = file.take(len);

    std::io::copy(&mut stream, &mut hasher).context(IoSnafu {})?;

    let hash = hasher.finalize();

    Ok(format!("{:X}", hash))
}

pub fn scan_pbo(path: &Path, base_path: &Path) -> Result<File, Error> {
    let mut file = BufReader::new(std::fs::File::open(path).context(IoSnafu)?);

    let mut parts = Vec::new();
    let pbo = crate::pbo::Pbo::read(&mut file).context(PboSnafu)?;
    let mut offset = 0;

    let length = pbo.input.seek(SeekFrom::End(0)).context(IoSnafu)?;
    pbo.input.seek(SeekFrom::Start(0)).context(IoSnafu)?;

    {
        let header_hash = generate_hash(pbo.input, pbo.header_len)?;
        offset += pbo.header_len;

        parts.push(Part {
            path: "$$HEADER$$".to_string(),
            length: pbo.header_len,
            start: 0,
            checksum: header_hash,
        });
    }

    // swifty, as always, does very strange things
    for entry in pbo.entries.iter().skip(1) {
        let hash = generate_hash(pbo.input, entry.data_size as u64)?;

        parts.push(Part {
            path: entry.filename.clone(),
            length: entry.data_size as u64,
            checksum: hash,
            start: offset,
        });

        offset += entry.data_size as u64;
    }

    {
        // TODO: this once panicked due to underflow.
        let remaining_len = length - offset;

        let end_hash = generate_hash(pbo.input, remaining_len)?;
        parts.push(Part {
            path: "$$END$$".to_string(),
            length: remaining_len,
            checksum: end_hash,
            start: offset,
        });
    }

    let checksum = {
        let mut hasher = Md5::new();

        for part in &parts {
            hasher.update(&part.checksum);
        }

        format!("{:X}", hasher.finalize())
    };

    let path = RelativePathBuf::from_path(path.strip_prefix(base_path).unwrap()).unwrap();

    Ok(File {
        r#type: FileType::Pbo,
        path,
        parts,
        checksum,
        length,
    })
}

pub fn scan_file(path: &Path, base_path: &Path) -> Result<File, Error> {
    let file = std::fs::File::open(path).context(IoSnafu)?;
    let mut parts = Vec::new();

    let file_len = file.metadata().context(IoSnafu)?.len();

    let mut reader = BufReader::new(file);
    let mut pos = 0;

    while pos < file_len {
        let mut hasher = Md5::new();
        let mut stream = reader.by_ref().take(5000000);

        let pre_copy_pos = pos;
        let copied = std::io::copy(&mut stream, &mut hasher).context(IoSnafu {})?;
        pos += copied;

        let hash = hasher.finalize();

        parts.push(Part {
            checksum: format!("{:X}", hash),
            length: copied,
            path: format!(
                "{}_{}",
                path.components()
                    .last()
                    .unwrap()
                    .as_os_str()
                    .to_string_lossy(),
                pos
            ),
            start: pre_copy_pos,
        })
    }

    // final checksum generation
    // swifty hashes the checksum strings
    let mut hasher = Md5::new();

    for part in &parts {
        hasher.update(&part.checksum)
    }

    let path = RelativePathBuf::from_path(path.strip_prefix(base_path).unwrap()).unwrap();

    Ok(File {
        checksum: format!("{:X}", hasher.finalize()),
        length: pos,
        parts,
        path,
        r#type: FileType::File,
    })
}

fn recurse(path: &Path, base_path: &Path) -> Result<Vec<File>, Error> {
    println!("recursing into {:#?}", &path);

    let entries: Vec<_> = WalkDir::new(path)
        .into_iter()
        .filter_entry(|e| e.file_name() != OsStr::new("mod.srf"))
        .filter_map(|e| e.ok())
        .filter(|e| {
            // someday this spaghetti can just be replaced by Option::contains
            if let Some(is_dir) = e.metadata().ok().map(|metadata| metadata.is_dir()) {
                !is_dir
            } else {
                false
            }
        })
        .map(|entry| entry.path().to_owned())
        .collect();

    let files: Result<Vec<_>, _> = entries
        .par_iter()
        .map(|path| {
            let extension = path.extension();

            match extension {
                Some(extension) if extension == "pbo" => scan_pbo(path, base_path),
                _ => scan_file(path, base_path),
            }
        })
        .collect();

    files
}

pub fn scan_mod(path: &Path) -> Result<Mod, Error> {
    let mut files = recurse(path, path)?;

    files.sort_by(|a, b| {
        a.path
            .as_str()
            .to_uppercase()
            .cmp(&b.path.as_str().to_uppercase())
    });

    let checksum = {
        let mut hasher = Md5::new();

        for file in &files {
            hasher.update(&file.checksum);
            let relpath = file.path.as_str().to_lowercase().replace('\\', "/");
            hasher.update(relpath);
        }

        let output = hasher.finalize();
        Md5Digest::from_bytes(output.into())
    };

    Ok(Mod {
        name: path
            .components()
            .last()
            .unwrap()
            .as_os_str()
            .to_string_lossy()
            .to_lowercase(),
        checksum,
        files,
    })
}

fn read_legacy_srf_addon(line: &str) -> Result<(Mod, u32), Error> {
    let mut split = line.split(':');

    let r#type = split
        .next()
        .context(LegacySrfParseFailureSnafu {
            description: "addon line missing type",
        })?
        .to_string();

    if r#type != "ADDON" {
        panic!("wrong magic");
    }

    let name = split
        .next()
        .context(LegacySrfParseFailureSnafu {
            description: "addon line missing name",
        })?
        .to_string();

    let size = split
        .next()
        .context(LegacySrfParseFailureSnafu {
            description: "addon line missing size",
        })?
        .parse()
        .context(LegacySrfU32ParseFailureSnafu)?;

    let checksum_digest = split
        .next()
        .context(LegacySrfParseFailureSnafu {
            description: "addon line missing checksum",
        })?
        .to_string();

    let checksum = Md5Digest::new(&checksum_digest).context(DigestParseSnafu)?;

    Ok((
        Mod {
            name,
            checksum,
            files: Vec::new(),
        },
        size,
    ))
}

fn read_legacy_srf_part(line: &str) -> Result<Part, Error> {
    let mut split = line.split(':');

    let path = split
        .next()
        .context(LegacySrfParseFailureSnafu {
            description: "part line missing path",
        })?
        .to_string();

    let start: u64 = split
        .next()
        .context(LegacySrfParseFailureSnafu {
            description: "part line missing start",
        })?
        .parse()
        .context(LegacySrfU32ParseFailureSnafu)?;

    let length: u64 = split
        .next()
        .context(LegacySrfParseFailureSnafu {
            description: "part line missing length",
        })?
        .parse()
        .context(LegacySrfU32ParseFailureSnafu)?;

    let checksum = split
        .next()
        .context(LegacySrfParseFailureSnafu {
            description: "part line missing checksum",
        })?
        .to_string();

    Ok(Part {
        path,
        start,
        length,
        checksum,
    })
}

fn read_legacy_srf_file(
    line: &str,
    lines: &mut impl Iterator<Item = String>,
) -> Result<File, Error> {
    let mut split = line.split(':');

    let r#type = FileType::from_legacy_srf(split.next().context(LegacySrfParseFailureSnafu {
        description: "no first element",
    })?)?;

    let path = RelativePathBuf::from(
        split
            .next()
            .context(LegacySrfParseFailureSnafu {
                description: "file line missing path",
            })?
            .to_string(),
    );

    let length: u64 = split
        .next()
        .context(LegacySrfParseFailureSnafu {
            description: "file line missing length",
        })?
        .parse()
        .context(LegacySrfU32ParseFailureSnafu)?;

    let part_count: u32 = split
        .next()
        .context(LegacySrfParseFailureSnafu {
            description: "file line missing part count",
        })?
        .parse()
        .context(LegacySrfU32ParseFailureSnafu)?;

    let checksum = split
        .next()
        .context(LegacySrfParseFailureSnafu {
            description: "file line missing checksum",
        })?
        .to_string();

    let mut parts = Vec::new();

    for _ in 0..part_count {
        let line = lines.next().context(LegacySrfParseFailureSnafu {
            description: "part line missing",
        })?;

        parts.push(read_legacy_srf_part(&line)?);
    }

    Ok(File {
        r#type,
        path,
        length,
        checksum,
        parts,
    })
}

pub fn is_legacy_srf<I: Read + Seek>(input: &mut I) -> Result<bool, io::Error> {
    let start = input.stream_position()?;
    let mut buf = [0; 5];
    input.read_exact(&mut buf)?;

    let output = if String::from_utf8_lossy(&buf) == "ADDON" {
        true
    } else {
        false
    };

    input.seek(SeekFrom::Start(start))?;

    Ok(output)
}

pub fn deserialize_legacy_srf<I: BufRead + Seek>(input: &mut I) -> Result<Mod, Error> {
    // swifty's legacy srf format is stateful
    input.seek(SeekFrom::Start(0)).context(IoSnafu)?;
    let mut files = Vec::<File>::new();

    let mut iter = input.lines().map(|line| line.expect("input.lines failed"));

    let first_line = iter.next().context(LegacySrfParseFailureSnafu {
        description: "no first line",
    })?;

    let (addon, file_count) = read_legacy_srf_addon(&first_line)?;

    for _ in 0..file_count {
        let file = read_legacy_srf_file(
            &iter.next().context(LegacySrfParseFailureSnafu {
                description: "line missing",
            })?,
            &mut iter,
        )?;

        files.push(file);
    }

    Ok(addon)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::path::PathBuf;

    #[test]
    fn legacy_srf_test() {
        let input = include_bytes!("../test_files/legacy_format_mod.srf");
        let mut cursor = Cursor::new(input);
        let deserialized = deserialize_legacy_srf(&mut cursor).unwrap();

        assert_eq!(deserialized.name, "@lambs_danger");
        assert_eq!(
            deserialized.checksum,
            Md5Digest::new("44C1B8021822F80E1E560689D2AAB0BF").unwrap()
        );
    }

    #[test]
    fn gen_srf_test() {
        let project_root = env!("CARGO_MANIFEST_DIR");
        let r#mod = scan_mod(
            &[project_root, "test_files", "@ace"]
                .iter()
                .collect::<PathBuf>(),
        )
        .unwrap();

        assert_eq!(
            r#mod.checksum,
            Md5Digest::new("787662722D70C36DF28CD1D5EE8D8E86").unwrap()
        );
    }
}
