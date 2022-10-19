use md5::{Digest, Md5};
use rayon::prelude::*;
use relative_path::RelativePathBuf;
use serde::{Deserialize, Deserializer, Serialize};
use snafu::{OptionExt, ResultExt, Snafu, Whatever};
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
}

impl FileType {
    fn from_legacy_srf(legacy_type: &str) -> Self {
        match legacy_type {
            "PBO" => Self::Pbo,
            "FILE" => Self::File,
            _ => panic!("unknown legacy file type"),
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
    pub checksum: String,
    pub files: Vec<File>,
}

impl Mod {
    pub fn generate_invalid(remote: &Self) -> Self {
        Self {
            checksum: "INVALID".into(),
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
    let mut file = BufReader::new(std::fs::File::open(&path).context(IoSnafu {})?);

    let mut parts = Vec::new();
    dbg!("scan pbo {}", path);
    let pbo = crate::pbo::Pbo::read(&mut file).context(PboSnafu {})?;
    let mut offset = 0;

    let length = pbo.input.seek(SeekFrom::End(0)).context(IoSnafu {})?;
    pbo.input.seek(SeekFrom::Start(0)).context(IoSnafu {})?;

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
    for entry in &pbo.entries {
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
    let file = std::fs::File::open(&path).context(IoSnafu {})?;
    let mut parts = Vec::new();

    let file_len = file.metadata().context(IoSnafu {})?.len();

    let mut reader = std::io::BufReader::new(file);
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
        .filter_map(|e| e.ok())
        .filter(|e| {
            // someday this spaghetti can just be replaced by Option::contains
            if let Some(is_dir) = e
                .metadata()
                .ok()
                .and_then(|metadata| Some(metadata.is_dir()))
            {
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
                Some(extension) if extension == "pbo" => scan_pbo(&path, base_path),
                _ => scan_file(&path, base_path),
            }
        })
        .collect();

    Ok(files?)
}

// FIXME: ditch whatever errors
pub fn scan_mod(path: &Path) -> Result<Mod, Error> {
    let mut files = recurse(path, path)?;

    files.sort_by(|a, b| {
        a.path
            .to_string()
            .to_lowercase()
            .cmp(&b.path.to_string().to_lowercase())
    });

    let checksum = {
        let mut hasher = Md5::new();

        for file in &files {
            hasher.update(&file.checksum);
            hasher.update(file.path.to_string().to_lowercase().replace("\\", "/"));
        }

        format!("{:X}", hasher.finalize())
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

fn read_legacy_srf_addon(line: &str) -> Result<(Mod, u32), Whatever> {
    let mut split = line.split(':');

    let r#type = split
        .next()
        .with_whatever_context(|| "no first element?")?
        .to_string();

    if r#type != "ADDON" {
        panic!("wrong magic");
    }

    let name = split
        .next()
        .with_whatever_context(|| "no second element?")?
        .to_string();

    let size = split
        .next()
        .with_whatever_context(|| "no third element?")?
        .parse()
        .with_whatever_context(|_| "failed to parse size")?;
    let checksum = split
        .next()
        .with_whatever_context(|| "no fourth element?")?
        .to_string();

    Ok((
        Mod {
            name,
            checksum,
            files: Vec::new(),
        },
        size,
    ))
}

fn read_legacy_srf_part(line: &str) -> Result<Part, Whatever> {
    let mut split = line.split(':');

    let path = split
        .next()
        .with_whatever_context(|| "no first element")?
        .to_string();

    let start: u64 = split
        .next()
        .with_whatever_context(|| "no second element")?
        .parse()
        .with_whatever_context(|_| "start was not a u64")?;

    let length: u64 = split
        .next()
        .with_whatever_context(|| "no third element")?
        .parse()
        .with_whatever_context(|_| "start was not a u64")?;

    let checksum = split
        .next()
        .with_whatever_context(|| "no fourth element")?
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
) -> Result<File, Whatever> {
    let mut split = line.split(':');

    let r#type =
        FileType::from_legacy_srf(split.next().with_whatever_context(|| "no first element")?);

    let path = RelativePathBuf::from(
        split
            .next()
            .with_whatever_context(|| "no second element")?
            .to_string(),
    );

    let length: u64 = split
        .next()
        .with_whatever_context(|| "no third element")?
        .parse()
        .with_whatever_context(|_| "length was not a u64")?;

    let part_count: u32 = split
        .next()
        .with_whatever_context(|| "no fourth element")?
        .parse()
        .with_whatever_context(|_| "file_count was not a u32")?;

    let checksum = split
        .next()
        .with_whatever_context(|| "no fifth element")?
        .to_string();

    let mut parts = Vec::new();

    for _ in 0..part_count {
        let line = lines.next().with_whatever_context(|| "missing line")?;

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

pub fn deserialize_legacy_srf<I: BufRead + Seek>(input: &mut I) -> Result<Mod, Whatever> {
    // swifty's legacy srf format is stateful
    input
        .seek(SeekFrom::Start(0))
        .with_whatever_context(|_| "failed to rewind file")?;
    let mut files = Vec::<File>::new();

    let mut iter = input.lines().map(|line| line.expect("input.lines failed"));

    let first_line = iter.next().with_whatever_context(|| "no first line")?;

    let (addon, file_count) = read_legacy_srf_addon(&first_line)?;

    for _ in 0..file_count {
        let file = read_legacy_srf_file(
            &iter.next().with_whatever_context(|| "missing lines")?,
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

    /*
    #[test]
    fn legacy_srf_test() {
        let input = include_bytes!("mod.srf");
        let mut cursor = Cursor::new(input);
        let deserialized = deserialize_legacy_srf(&mut cursor).unwrap();
        dbg!(deserialized);
    }
     */
}
