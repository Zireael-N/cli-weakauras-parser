use super::{EmbeddedTypeTag, TypeTag, MINOR};
use crate::{
    macros::check_recursion,
    value::{LuaMapKey, LuaValue, Map},
};
use std::convert::TryInto;

const TYPE_TAG_SHIFT: u8 = 3;
const EMBEDDED_TYPE_TAG_SHIFT: u8 = 2;
const EMBEDDED_LEN_SHIFT: u8 = 4;

fn required_bytes(v: u64) -> u8 {
    match v {
        0..=255 => 1,
        256..=65_535 => 2,
        65_536..=16_777_215 => 3,
        16_777_216..=4_294_967_295 => 4,
        _ => 7,
    }
}

pub struct Serializer {
    remaining_depth: usize,
    result: Vec<u8>,

    string_refs: Map<String, usize>,
}

impl Serializer {
    pub fn serialize(
        value: &LuaValue,
        approximate_len: Option<usize>,
    ) -> Result<Vec<u8>, &'static str> {
        let mut serializer = Self {
            remaining_depth: 128,
            result: Vec::with_capacity(approximate_len.unwrap_or(1024)),

            string_refs: Map::new(),
        };

        serializer.result.push(MINOR);
        serializer.serialize_helper(value)?;

        Ok(serializer.result)
    }

    fn serialize_helper(&mut self, value: &LuaValue) -> Result<(), &'static str> {
        match *value {
            LuaValue::Null => self.result.push(TypeTag::Null.to_u8() << TYPE_TAG_SHIFT),
            LuaValue::Boolean(b) => {
                if b {
                    self.result.push(TypeTag::True.to_u8() << TYPE_TAG_SHIFT);
                } else {
                    self.result.push(TypeTag::False.to_u8() << TYPE_TAG_SHIFT);
                }
            }
            LuaValue::String(ref s) => self.serialize_string(s)?,
            LuaValue::Number(n) => self.serialize_number(n),
            LuaValue::Array(ref v) => self.serialize_slice(v)?,
            LuaValue::Map(ref m) => self.serialize_map(m)?,
        }

        Ok(())
    }

    fn serialize_number(&mut self, value: f64) {
        const MAX_7_BIT: f64 = (2i64.pow(56) - 1) as f64;

        #[allow(clippy::manual_range_contains)]
        if value.fract() != 0.0 || (value < -MAX_7_BIT || value > MAX_7_BIT) {
            self.result.push(TypeTag::Float.to_u8() << TYPE_TAG_SHIFT);
            self.result.extend_from_slice(&value.to_be_bytes());
        } else {
            // SAFETY:
            // 1) for infinity and NaNs, `f64::fract()` returns `f64::NAN`;
            // 2) `value` does not have a fractional part;
            // 3) `value` is within i64::MIN..=i64::MAX range.
            let value = unsafe { value.to_int_unchecked::<i64>() };

            if value > -4096 && value < 4096 {
                if value >= 0 && value < 128 {
                    self.result.push(((value as u8) << 1) | 1);
                } else {
                    let (value, neg_bit) = if value < 0 {
                        (-value, 1 << TYPE_TAG_SHIFT)
                    } else {
                        (value, 0)
                    };

                    let value = (value << 4) | neg_bit | 4;
                    self.result.push(value as u8);
                    self.result.push((value >> 8) as u8);
                }
            } else {
                let (value, neg_bit) = if value < 0 {
                    ((-value) as u64, 1)
                } else {
                    (value as u64, 0)
                };

                match required_bytes(value) {
                    2 => {
                        self.result
                            .push((TypeTag::Int16Pos.to_u8() + neg_bit) << TYPE_TAG_SHIFT);
                        self.serialize_int(value, 2);
                    }
                    3 => {
                        self.result
                            .push((TypeTag::Int24Pos.to_u8() + neg_bit) << TYPE_TAG_SHIFT);
                        self.serialize_int(value, 3);
                    }
                    4 => {
                        self.result
                            .push((TypeTag::Int32Pos.to_u8() + neg_bit) << TYPE_TAG_SHIFT);
                        self.serialize_int(value, 4);
                    }
                    _ => {
                        self.result
                            .push((TypeTag::Int64Pos.to_u8() + neg_bit) << TYPE_TAG_SHIFT);
                        self.serialize_int(value, 7);
                    }
                }
            }
        }
    }

    fn serialize_int(&mut self, value: u64, len: usize) {
        let bytes = value.to_be_bytes();
        self.result.extend_from_slice(&bytes[bytes.len() - len..]);
    }

    fn serialize_string(&mut self, value: &str) -> Result<(), &'static str> {
        match self.string_refs.get(value) {
            Some(index) => {
                let index: u64 = (*index).try_into().unwrap();
                match required_bytes(index) {
                    1 => {
                        self.result.push(TypeTag::StrRef8.to_u8() << TYPE_TAG_SHIFT);
                        self.serialize_int(index, 1);
                    }
                    2 => {
                        self.result
                            .push(TypeTag::StrRef16.to_u8() << TYPE_TAG_SHIFT);
                        self.serialize_int(index, 2);
                    }
                    3 => {
                        self.result
                            .push(TypeTag::StrRef24.to_u8() << TYPE_TAG_SHIFT);
                        self.serialize_int(index, 3);
                    }
                    _ => return Err("Can't serialize: more than 2^24 different strings"),
                }
            }
            None => {
                let len = value.len();

                if len < 16 {
                    self.result.push(
                        (EmbeddedTypeTag::Str.to_u8() << EMBEDDED_TYPE_TAG_SHIFT)
                            | ((len as u8) << EMBEDDED_LEN_SHIFT)
                            | 2,
                    );
                } else {
                    let len: u64 = len.try_into().unwrap();
                    match required_bytes(len) {
                        1 => {
                            self.result.push(TypeTag::Str8.to_u8() << TYPE_TAG_SHIFT);
                            self.serialize_int(len, 1);
                        }
                        2 => {
                            self.result.push(TypeTag::Str16.to_u8() << TYPE_TAG_SHIFT);
                            self.serialize_int(len, 2);
                        }
                        3 => {
                            self.result.push(TypeTag::Str24.to_u8() << TYPE_TAG_SHIFT);
                            self.serialize_int(len, 3);
                        }
                        _ => return Err("Can't serialize: string is too large"),
                    }
                }

                if len > 2 {
                    self.string_refs
                        .insert(value.into(), self.string_refs.len() + 1);
                }

                self.result.extend_from_slice(value.as_bytes());
            }
        }

        Ok(())
    }

    fn serialize_map(&mut self, map: &Map<LuaMapKey, LuaValue>) -> Result<(), &'static str> {
        let len = map.len();
        if len < 16 {
            self.result.push(
                (EmbeddedTypeTag::Map.to_u8() << EMBEDDED_TYPE_TAG_SHIFT)
                    | ((len as u8) << EMBEDDED_LEN_SHIFT)
                    | 2,
            );
        } else {
            let len: u64 = len.try_into().unwrap();
            match required_bytes(len) {
                1 => {
                    self.result.push(TypeTag::Map8.to_u8() << TYPE_TAG_SHIFT);
                    self.serialize_int(len, 1);
                }
                2 => {
                    self.result.push(TypeTag::Map16.to_u8() << TYPE_TAG_SHIFT);
                    self.serialize_int(len, 2);
                }
                3 => {
                    self.result.push(TypeTag::Map24.to_u8() << TYPE_TAG_SHIFT);
                    self.serialize_int(len, 3);
                }
                _ => return Err("Can't serialize: map is too large"),
            }
        }

        for (key, value) in map {
            check_recursion!(self, {
                self.serialize_helper(key.as_value())?;
                self.serialize_helper(value)?;
            });
        }

        Ok(())
    }

    fn serialize_slice(&mut self, slice: &[LuaValue]) -> Result<(), &'static str> {
        let len = slice.len();
        if len < 16 {
            self.result.push(
                (EmbeddedTypeTag::Array.to_u8() << EMBEDDED_TYPE_TAG_SHIFT)
                    | ((len as u8) << EMBEDDED_LEN_SHIFT)
                    | 2,
            );
        } else {
            let len: u64 = len.try_into().unwrap();
            match required_bytes(len) {
                1 => {
                    self.result.push(TypeTag::Array8.to_u8() << TYPE_TAG_SHIFT);
                    self.serialize_int(len, 1);
                }
                2 => {
                    self.result.push(TypeTag::Array16.to_u8() << TYPE_TAG_SHIFT);
                    self.serialize_int(len, 2);
                }
                3 => {
                    self.result.push(TypeTag::Array24.to_u8() << TYPE_TAG_SHIFT);
                    self.serialize_int(len, 3);
                }
                _ => return Err("Can't serialize: array is too large"),
            }
        }

        for el in slice {
            check_recursion!(self, {
                self.serialize_helper(el)?;
            });
        }

        Ok(())
    }
}
