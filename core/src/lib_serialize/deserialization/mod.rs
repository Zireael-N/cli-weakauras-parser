mod reader;

use super::{EmbeddedTypeTag, MINOR, TypeTag};
use crate::{
    macros::check_recursion,
    value::{LuaMapKey, LuaValue, Map},
};
use reader::SliceReader;
use std::convert::{TryFrom, TryInto};

pub struct Deserializer<'s> {
    remaining_depth: usize,
    reader: SliceReader<'s>,

    table_refs: Vec<LuaValue>,
    string_refs: Vec<String>,
}

impl<'s> Deserializer<'s> {
    pub fn from_slice(v: &'s [u8]) -> Self {
        Self {
            remaining_depth: 128,
            reader: SliceReader::new(v),

            table_refs: Vec::new(),
            string_refs: Vec::new(),
        }
    }

    /// Returns an array of deserialized values
    #[allow(dead_code)]
    pub fn deserialize(mut self) -> Result<Vec<LuaValue>, &'static str> {
        match self.reader.read_u8() {
            Some(MINOR) => (),
            _ => return Err("Invalid serialized data"),
        }

        let mut result = Vec::new();

        while let Some(v) = self.deserialize_helper()? {
            result.push(v);
        }

        Ok(result)
    }

    /// Returns the first deserialized value
    #[allow(dead_code)]
    pub fn deserialize_first(mut self) -> Result<Option<LuaValue>, &'static str> {
        match self.reader.read_u8() {
            Some(MINOR) => (),
            _ => return Err("Invalid serialized data"),
        }

        self.deserialize_helper()
    }

    fn deserialize_helper(&mut self) -> Result<Option<LuaValue>, &'static str> {
        match self.reader.read_u8() {
            None => Ok(None),
            Some(value) => {
                if value & 1 == 1 {
                    // `NNNN NNN1`: a 7 bit non-negative int
                    Ok(Some(LuaValue::Number((value >> 1) as f64)))
                } else if value & 3 == 2 {
                    // * `CCCC TT10`: a 2 bit type index and 4 bit count (strlen, #tab, etc.)
                    //     * Followed by the type-dependent payload
                    let tag = EmbeddedTypeTag::from_u8((value & 0x0F) >> 2)
                        .ok_or("Invalid embedded tag")?;
                    let len = value >> 4;

                    self.deserialize_embedded(tag, len).map(Option::Some)
                } else if value & 7 == 4 {
                    // * `NNNN S100`: the lower four bits of a 12 bit int and 1 bit for its sign
                    //     * Followed by a byte for the upper bits
                    let next_byte = self.reader.read_u8().ok_or("Unexpected EOF")? as u16;
                    let packed = (next_byte << 8) + value as u16;

                    Ok(Some(LuaValue::Number(if value & 15 == 12 {
                        -((packed >> 4) as f64)
                    } else {
                        (packed >> 4) as f64
                    })))
                } else {
                    // * `TTTT T000`: a 5 bit type index
                    //     * Followed by the type-dependent payload, including count(s) if needed
                    let tag = TypeTag::from_u8(value >> 3).ok_or("Invalid tag")?;

                    self.deserialize_one(tag).map(Option::Some)
                }
            }
        }
    }

    #[inline(always)]
    fn extract_value(&mut self) -> Result<LuaValue, &'static str> {
        match self.deserialize_helper() {
            Ok(Some(value)) => Ok(value),
            Ok(None) => Err("Unexpected EOF"),
            Err(e) => Err(e),
        }
    }

    fn deserialize_embedded(
        &mut self,
        tag: EmbeddedTypeTag,
        len: u8,
    ) -> Result<LuaValue, &'static str> {
        match tag {
            EmbeddedTypeTag::Str => self.deserialize_string(len.into()),
            EmbeddedTypeTag::Map => self.deserialize_map(len.into()),
            EmbeddedTypeTag::Array => self.deserialize_array(len.into()),
            // For MIXED, the 4-bit count contains two 2-bit counts that are one less than the true count.
            EmbeddedTypeTag::Mixed => {
                self.deserialize_mixed(((len & 3) + 1).into(), ((len >> 2) + 1).into())
            }
        }
    }

    fn deserialize_one(&mut self, tag: TypeTag) -> Result<LuaValue, &'static str> {
        match tag {
            TypeTag::Null => Ok(LuaValue::Null),

            TypeTag::Int16Pos => self.deserialize_int(2).map(|v| LuaValue::Number(v as f64)),
            TypeTag::Int16Neg => self
                .deserialize_int(2)
                .map(|v| LuaValue::Number(-(v as f64))),
            TypeTag::Int24Pos => self.deserialize_int(3).map(|v| LuaValue::Number(v as f64)),
            TypeTag::Int24Neg => self
                .deserialize_int(3)
                .map(|v| LuaValue::Number(-(v as f64))),
            TypeTag::Int32Pos => self.deserialize_int(4).map(|v| LuaValue::Number(v as f64)),
            TypeTag::Int32Neg => self
                .deserialize_int(4)
                .map(|v| LuaValue::Number(-(v as f64))),
            TypeTag::Int64Pos => self.deserialize_int(7).map(|v| LuaValue::Number(v as f64)),
            TypeTag::Int64Neg => self
                .deserialize_int(7)
                .map(|v| LuaValue::Number(-(v as f64))),

            TypeTag::Float => self.deserialize_f64().map(LuaValue::Number),
            TypeTag::FloatStrPos => self.deserialize_f64_from_str().map(LuaValue::Number),
            TypeTag::FloatStrNeg => self
                .deserialize_f64_from_str()
                .map(|v| LuaValue::Number(-v)),

            TypeTag::True => Ok(LuaValue::Boolean(true)),
            TypeTag::False => Ok(LuaValue::Boolean(false)),

            TypeTag::Str8 => {
                let len = self.reader.read_u8().ok_or("Unexpected EOF")?;
                self.deserialize_string(len.into())
            }
            TypeTag::Str16 => {
                let len = self.deserialize_int(2)?;
                self.deserialize_string(len.try_into().unwrap())
            }
            TypeTag::Str24 => {
                let len = self.deserialize_int(3)?;
                self.deserialize_string(len.try_into().unwrap())
            }

            TypeTag::Map8 => {
                let len = self.reader.read_u8().ok_or("Unexpected EOF")?;
                self.deserialize_map(len.into())
            }
            TypeTag::Map16 => {
                let len = self.deserialize_int(2)?;
                self.deserialize_map(len.try_into().unwrap())
            }
            TypeTag::Map24 => {
                let len = self.deserialize_int(3)?;
                self.deserialize_map(len.try_into().unwrap())
            }

            TypeTag::Array8 => {
                let len = self.reader.read_u8().ok_or("Unexpected EOF")?;
                self.deserialize_array(len.into())
            }
            TypeTag::Array16 => {
                let len = self.deserialize_int(2)?;
                self.deserialize_array(len.try_into().unwrap())
            }
            TypeTag::Array24 => {
                let len = self.deserialize_int(3)?;
                self.deserialize_array(len.try_into().unwrap())
            }

            TypeTag::Mixed8 => {
                let array_len = self.reader.read_u8().ok_or("Unexpected EOF")?;
                let map_len = self.reader.read_u8().ok_or("Unexpected EOF")?;

                self.deserialize_mixed(array_len.into(), map_len.into())
            }
            TypeTag::Mixed16 => {
                let array_len = self.deserialize_int(2)?;
                let map_len = self.deserialize_int(2)?;

                self.deserialize_mixed(array_len.try_into().unwrap(), map_len.try_into().unwrap())
            }
            TypeTag::Mixed24 => {
                let array_len = self.deserialize_int(3)?;
                let map_len = self.deserialize_int(3)?;

                self.deserialize_mixed(array_len.try_into().unwrap(), map_len.try_into().unwrap())
            }

            TypeTag::StrRef8 => {
                let index = self.reader.read_u8().ok_or("Unexpected EOF")? - 1;
                match self.string_refs.get(usize::from(index)) {
                    None => Err("Invalid string reference"),
                    Some(s) => Ok(LuaValue::String(s.clone())),
                }
            }
            TypeTag::StrRef16 => {
                let index = self.deserialize_int(2)? - 1;
                match self.string_refs.get(usize::try_from(index).unwrap()) {
                    None => Err("Invalid string reference"),
                    Some(s) => Ok(LuaValue::String(s.clone())),
                }
            }
            TypeTag::StrRef24 => {
                let index = self.deserialize_int(3)? - 1;
                match self.string_refs.get(usize::try_from(index).unwrap()) {
                    None => Err("Invalid string reference"),
                    Some(s) => Ok(LuaValue::String(s.clone())),
                }
            }

            TypeTag::MapRef8 => {
                let index = self.reader.read_u8().ok_or("Unexpected EOF")? - 1;
                match self.table_refs.get(usize::from(index)) {
                    None => Err("Invalid table reference"),
                    Some(v) => Ok(v.clone()),
                }
            }
            TypeTag::MapRef16 => {
                let index = self.deserialize_int(2)? - 1;
                match self.table_refs.get(usize::try_from(index).unwrap()) {
                    None => Err("Invalid table reference"),
                    Some(v) => Ok(v.clone()),
                }
            }
            TypeTag::MapRef24 => {
                let index = self.deserialize_int(3)? - 1;
                match self.table_refs.get(usize::try_from(index).unwrap()) {
                    None => Err("Invalid table reference"),
                    Some(v) => Ok(v.clone()),
                }
            }
        }
    }

    fn deserialize_string(&mut self, len: usize) -> Result<LuaValue, &'static str> {
        match self.reader.read_string(len) {
            None => Err("Unexpected EOF"),
            Some(s) => {
                let s = s.into_owned();
                if len > 2 {
                    self.string_refs.push(s.clone());
                }

                Ok(LuaValue::String(s))
            }
        }
    }

    fn deserialize_f64(&mut self) -> Result<f64, &'static str> {
        match self.reader.read_f64() {
            None => Err("Unexpected EOF"),
            Some(v) => Ok(v),
        }
    }

    fn deserialize_f64_from_str(&mut self) -> Result<f64, &'static str> {
        let len = self.reader.read_u8().ok_or("Unexpected EOF")?;

        match self.reader.read_bytes(len.into()) {
            None => Err("Unexpected EOF"),
            Some(bytes) => std::str::from_utf8(bytes)
                .ok()
                .and_then(|s| s.parse::<f64>().ok())
                .ok_or("Cannot parse a number"),
        }
    }

    fn deserialize_int(&mut self, bytes: usize) -> Result<u64, &'static str> {
        match self.reader.read_int(bytes) {
            None => Err("Unexpected EOF"),
            Some(v) => Ok(v),
        }
    }

    fn deserialize_map(&mut self, len: usize) -> Result<LuaValue, &'static str> {
        let mut m = Map::new();

        for _ in 0..len {
            check_recursion!(self, {
                let (key, value) = (self.extract_value()?, self.extract_value()?);

                m.insert(LuaMapKey::from_value(key)?, value);
            });
        }

        let m = LuaValue::Map(m);
        self.table_refs.push(m.clone());
        Ok(m)
    }

    fn deserialize_array(&mut self, len: usize) -> Result<LuaValue, &'static str> {
        let mut v = Vec::new();

        for _ in 0..len {
            check_recursion!(self, {
                v.push(self.extract_value()?);
            });
        }

        let v = LuaValue::Array(v);
        self.table_refs.push(v.clone());
        Ok(v)
    }

    fn deserialize_mixed(
        &mut self,
        array_len: usize,
        map_len: usize,
    ) -> Result<LuaValue, &'static str> {
        let mut m = Map::new();

        for i in 1..=array_len {
            check_recursion!(self, {
                let el = self.extract_value()?;
                m.insert(
                    LuaMapKey::from_value(LuaValue::Number(i as f64)).unwrap(),
                    el,
                );
            });
        }

        for _ in 0..map_len {
            check_recursion!(self, {
                let (key, value) = (self.extract_value()?, self.extract_value()?);

                m.insert(LuaMapKey::from_value(key)?, value);
            });
        }

        let m = LuaValue::Map(m);
        self.table_refs.push(m.clone());
        Ok(m)
    }
}
