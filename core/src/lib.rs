mod ace_serialize;
mod lib_serialize;
mod macros;
mod value;

use ace_serialize::{Deserializer as LegacyDeserializer, Serializer as LegacySerializer};
use lib_serialize::{Deserializer, Serializer};
pub use value::LuaValue;

use std::borrow::Cow;

const MAX_SIZE: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StringVersion {
    Huffman,             // base64
    Deflate,             // ! + base64
    BinarySerialization, // !WA:\d+! + base64
}

/// Takes a string encoded by WeakAuras and returns
/// a Vec of [LuaValues](enum.LuaValue.html).
pub fn decode(mut data: &str) -> Result<Option<LuaValue>, &'static str> {
    let version = if data.starts_with("!WA:2!") {
        data = &data[6..];
        StringVersion::BinarySerialization
    } else if data.starts_with('!') {
        data = &data[1..];
        StringVersion::Deflate
    } else {
        StringVersion::Huffman
    };

    let data = wa_base64::decode(trim_ascii_from_end_of_str(data))?;
    let decoded = if version == StringVersion::Huffman {
        huffman::decompress(&data)
    } else {
        use flate2::read::DeflateDecoder;
        use std::io::prelude::*;

        let mut result = Vec::new();
        let mut inflater = DeflateDecoder::new(&data[..]).take(MAX_SIZE as u64);

        inflater
            .read_to_end(&mut result)
            .map_err(|_| "failed to INFLATE")
            .and_then(|_| {
                if result.len() < MAX_SIZE {
                    Ok(())
                } else {
                    match inflater.into_inner().bytes().next() {
                        Some(_) => Err("compressed data is too large"),
                        None => Ok(()),
                    }
                }
            })
            .map(|_| Cow::from(result))
    }?;

    if version == StringVersion::BinarySerialization {
        Deserializer::from_slice(&decoded).deserialize_first()
    } else {
        LegacyDeserializer::from_str(&String::from_utf8_lossy(&decoded)).deserialize_first()
    }
}

/// Takes a [LuaValue](enum.LuaValue.html) and returns
/// a string that can be decoded by WeakAuras.
pub fn encode(value: &LuaValue, format: StringVersion) -> Result<String, &'static str> {
    let (serialized, prefix) = match format {
        StringVersion::Deflate => (
            LegacySerializer::serialize(value, None).map(|v| v.into_bytes()),
            "!",
        ),
        StringVersion::BinarySerialization => (Serializer::serialize(value, None), "!WA:2!"),
        _ => unimplemented!(),
    };

    serialized
        .and_then(|serialized| {
            use flate2::{read::DeflateEncoder, Compression};
            use std::io::prelude::*;

            let mut result = Vec::new();
            let mut deflater = DeflateEncoder::new(serialized.as_slice(), Compression::best());

            deflater
                .read_to_end(&mut result)
                .map(|_| result)
                .map_err(|_| "failed to DEFLATE")
        })
        .and_then(|compressed| wa_base64::encode_with_prefix(&compressed, prefix))
}

// Borrowed from https://doc.rust-lang.org/std/primitive.slice.html#method.trim_ascii_end.
// As of Rust 1.76 it's nightly-only. Tracking issue: https://github.com/rust-lang/rust/issues/94035
#[inline]
const fn trim_ascii_from_end_of_slice(slice: &[u8]) -> &[u8] {
    let mut bytes = slice;
    // Note: A pattern matching based approach (instead of indexing) allows
    // making the function const.
    while let [rest @ .., last] = bytes {
        if last.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }
    bytes
}

// Borrowed from https://doc.rust-lang.org/std/primitive.str.html#method.trim_ascii_end.
// As of Rust 1.76 it's nightly-only. Tracking issue: https://github.com/rust-lang/rust/issues/94035
#[inline]
const fn trim_ascii_from_end_of_str(s: &str) -> &str {
    // SAFETY: Removing ASCII characters from a `&str` does not invalidate
    // UTF-8.
    unsafe { core::str::from_utf8_unchecked(trim_ascii_from_end_of_slice(s.as_bytes())) }
}
