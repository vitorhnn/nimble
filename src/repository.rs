use serde::{Deserialize, Deserializer, Serialize};
use snafu::prelude::*;
use std::{fmt::Display, net::IpAddr, str::FromStr};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Error while requesting repository data: {}", source))]
    Http {
        url: String,

        #[snafu(source(from(ureq::Error, Box::new)))]
        source: Box<ureq::Error>,
    },
    #[snafu(display("Error while deserializing: {}", source))]
    Deserialization { source: std::io::Error },
}

pub fn deserialize_number_from_string<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr + serde::Deserialize<'de>,
    <T as FromStr>::Err: Display,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrInt<T> {
        String(String),
        Number(T),
    }

    match StringOrInt::<T>::deserialize(deserializer)? {
        StringOrInt::String(s) => s.parse::<T>().map_err(serde::de::Error::custom),
        StringOrInt::Number(i) => Ok(i),
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")] // this particular file is camelcase for reasons
pub struct Mod {
    pub mod_name: String,
    #[serde(rename = "checkSum")] // why
    pub checksum: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")] // this particular file is camelcase for reasons
pub struct BasicAuth {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")] // this particular file is camelcase for reasons
pub struct Server {
    name: String,
    address: IpAddr,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    port: u16,
    password: String,
    battle_eye: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")] // this particular file is camelcase for reasons
pub struct Repository {
    pub repo_name: String,
    pub checksum: String,
    pub required_mods: Vec<Mod>,
    pub optional_mods: Vec<Mod>,
    pub client_parameters: String,
    pub repo_basic_authentication: Option<BasicAuth>,
    pub version: String,
    pub servers: Vec<Server>,
}

pub fn replicate_remote_repo_info(remote: &Repository) -> Repository {
    Repository {
        required_mods: vec![],
        optional_mods: vec![],
        checksum: "INVALID".into(),
        ..remote.clone()
    }
}

pub fn get_repository_info(agent: &mut ureq::Agent, url: &str) -> Result<Repository, Error> {
    agent
        .get(url)
        .call()
        .context(HttpSnafu { url })?
        .into_json()
        .context(DeserializationSnafu)
}
