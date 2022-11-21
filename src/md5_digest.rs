use std::fmt::{Debug, Formatter};
use serde::{Serialize, Deserialize, Serializer, Deserializer};

#[derive(Hash, PartialEq, Eq, Clone)]
pub struct Md5Digest {
    inner: [u8; 16],
}

impl Md5Digest {
   pub fn new(digest: &str) -> Self {
       let mut inner = [0; 16];
       hex::decode_to_slice(digest, &mut inner);
       Self {
           inner,
       }
   }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self {
            inner: bytes,
        }
    }
}

impl Default for Md5Digest {
    fn default() -> Self {
        Self {
            inner: [0; 16],
        }
    }
}

impl Serialize for Md5Digest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
        let digest = hex::encode_upper(&self.inner);

        serializer.serialize_str(&digest)
    }
}

impl<'de> Deserialize<'de> for Md5Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: Deserializer<'de> {
        let digest = String::deserialize(deserializer)?;

        let mut inner = [0; 16];
        hex::decode_to_slice(digest, &mut inner).map_err(serde::de::Error::custom)?;

        Ok(Self::from_bytes(inner))
    }
}

impl Debug for Md5Digest {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Md5Digest")
            .field("inner", &hex::encode_upper(&self.inner))
            .finish()
    }
}