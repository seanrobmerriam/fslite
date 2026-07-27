//! `serde(with = "bytes_b64")` helper: encodes `Vec<u8>` fields as base64
//! strings instead of serde's default JSON array-of-numbers, so the wire
//! format of every payload-carrying `Command`/`CommandOutput` variant is a
//! normal string, not a giant numeric array.

use base64::Engine;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    base64::engine::general_purpose::STANDARD
        .encode(bytes)
        .serialize(serializer)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let encoded = String::deserialize(deserializer)?;
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(serde::de::Error::custom)
}
