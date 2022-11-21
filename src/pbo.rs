use std::{
    collections::HashMap,
    ffi::CStr,
    io::{BufRead, Seek},
};

use byteorder::{LittleEndian, ReadBytesExt};
use snafu::{ResultExt, Snafu};

#[derive(Debug)]
pub struct Pbo<I: BufRead + Seek> {
    pub input: I,
    pub header_len: u64,
    pub extensions: HashMap<String, String>,
    pub entries: Vec<PboEntry>,
}

#[derive(Debug, PartialEq)]
pub enum EntryType {
    Vers,
    Cprs,
    Enco,
    None,
}

#[derive(Debug)]
pub struct PboEntry {
    pub filename: String,
    pub r#type: EntryType,
    pub original_size: u32,
    pub offset: u32,
    pub timestamp: u32,
    pub data_size: u32,
}

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("io error: {}", source))]
    Io { source: std::io::Error },
}

fn read_string<I: BufRead + Seek>(input: &mut I) -> Result<String, Error> {
    let mut buf = Vec::new();

    input.read_until(b'\0', &mut buf).context(IoSnafu {})?;

    let str = unsafe { CStr::from_bytes_with_nul_unchecked(&buf) }.to_string_lossy();

    Ok(str.to_string())
}

impl PboEntry {
    fn read<I: BufRead + Seek>(input: &mut I) -> Result<Self, Error> {
        let filename = read_string(input)?;

        let r#type = input.read_u32::<LittleEndian>().context(IoSnafu {})?;

        let r#type = match r#type {
            0x56657273 => EntryType::Vers,
            0x43707273 => EntryType::Cprs,
            0x456e6372 => EntryType::Enco,
            0x00000000 => EntryType::None,
            _ => panic!(),
        };

        let original_size = input.read_u32::<LittleEndian>().context(IoSnafu {})?;
        let offset = input.read_u32::<LittleEndian>().context(IoSnafu {})?;
        let timestamp = input.read_u32::<LittleEndian>().context(IoSnafu {})?;
        let data_size = input.read_u32::<LittleEndian>().context(IoSnafu {})?;

        Ok(PboEntry {
            filename,
            r#type,
            original_size,
            offset,
            timestamp,
            data_size,
        })
    }
}

fn read_extensions<I: BufRead + Seek>(input: &mut I) -> Result<HashMap<String, String>, Error> {
    let mut output_map = HashMap::new();

    loop {
        let key = read_string(input)?;
        if key.is_empty() {
            break;
        }

        let value = read_string(input)?;
        output_map.insert(key, value);
    }

    Ok(output_map)
}

impl<I: BufRead + Seek> Pbo<I> {
    pub fn read(mut input: I) -> Result<Self, Error> {
        let mut extensions = HashMap::new();

        let mut entries = Vec::new();

        loop {
            let entry = PboEntry::read(&mut input)?;

            if entry.r#type == EntryType::None && entry.filename.is_empty() {
                break;
            }

            if entry.r#type == EntryType::Vers {
                extensions = read_extensions(&mut input)?;
            }

            entries.push(entry);
        }

        let header_len = input.stream_position().unwrap();

        Ok(Pbo {
            input,
            header_len,
            extensions,
            entries,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_check() {
        dbg!(Pbo::read(&mut std::io::BufReader::new(
            std::fs::File::open(
                "/home/vitorhnn/arma_crap/mods/@ACE/addons/ace_advanced_ballistics.pbo",
            )
            .unwrap(),
        ))
        .unwrap());
        //Pbo::read(&mut std::io::BufReader::new(std::fs::File::open("/home/vitorhnn/arma_crap/mods/@ACE/addons/ace_advanced_ballistics.pbo.ace_3.13.6.60-8bd4922f.bisign").unwrap())).unwrap();
    }
}
