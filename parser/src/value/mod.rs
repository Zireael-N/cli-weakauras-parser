#[cfg(all(not(feature = "indexmap"), feature = "fnv"))]
pub use fnv::FnvHashMap as Map;
#[cfg(feature = "indexmap")]
pub use indexmap::IndexMap as Map;
#[cfg(not(any(feature = "indexmap", feature = "fnv")))]
pub use std::collections::BTreeMap as Map;

#[cfg(feature = "serde")]
use serde::{
    de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor},
    ser::{Serialize, Serializer},
};

#[derive(Debug, Clone)]
/// A tagged union representing all
/// possible values in Lua.
pub enum LuaValue {
    Map(Map<LuaMapKey, LuaValue>),
    Array(Vec<LuaValue>),
    String(String),
    Number(f64),
    Boolean(bool),
    Null,
}

#[derive(Clone)]
pub struct LuaMapKey(LuaValue);
impl LuaMapKey {
    #[inline(always)]
    pub fn as_value(&self) -> &LuaValue {
        &self.0
    }

    pub fn from_value(value: LuaValue) -> Result<Self, &'static str> {
        if let LuaValue::Null = value {
            Err("map key can't be null")
        } else {
            Ok(Self(value))
        }
    }
}

use core::hash::{Hash, Hasher};
impl Hash for LuaValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            LuaValue::Map(m) => state.write_usize(m as *const _ as usize),
            LuaValue::Array(v) => state.write_usize(v as *const _ as usize),
            LuaValue::String(s) => s.hash(state),
            LuaValue::Number(n) => state.write_u64(n.to_bits()),
            LuaValue::Boolean(b) => b.hash(state),
            LuaValue::Null => state.write_u8(0),
        }
    }
}

use std::cmp::Ordering;
impl PartialOrd for LuaValue {
    // Number > String > Boolean > Map > Null
    fn partial_cmp(&self, other: &LuaValue) -> Option<Ordering> {
        Some(match (self, other) {
            (LuaValue::Number(n1), LuaValue::Number(n2)) => {
                n1.partial_cmp(n2)
                    .unwrap_or_else(|| match (n1.is_nan(), n2.is_nan()) {
                        (true, false) => Ordering::Less,
                        (false, true) => Ordering::Greater,
                        _ => Ordering::Equal,
                    })
            }
            (LuaValue::Number(_), _) => Ordering::Greater,
            (_, LuaValue::Number(_)) => Ordering::Less,
            (LuaValue::String(s1), LuaValue::String(s2)) => s1.cmp(s2),
            (LuaValue::String(_), LuaValue::Boolean(_))
            | (LuaValue::String(_), LuaValue::Map(_))
            | (LuaValue::String(_), LuaValue::Array(_)) => Ordering::Greater,
            (LuaValue::Boolean(_), LuaValue::String(_))
            | (LuaValue::Map(_), LuaValue::String(_))
            | (LuaValue::Array(_), LuaValue::String(_)) => Ordering::Less,
            (LuaValue::Boolean(b1), LuaValue::Boolean(b2)) => b1.cmp(b2),
            (LuaValue::Boolean(_), LuaValue::Map(_))
            | (LuaValue::Boolean(_), LuaValue::Array(_)) => Ordering::Greater,
            (LuaValue::Map(_), LuaValue::Boolean(_))
            | (LuaValue::Array(_), LuaValue::Boolean(_)) => Ordering::Less,
            (LuaValue::Map(m1), LuaValue::Map(m2)) => {
                let p1 = m1 as *const _ as usize;
                let p2 = m2 as *const _ as usize;
                p1.cmp(&p2)
            }
            (LuaValue::Map(_), LuaValue::Array(_)) => Ordering::Greater,
            (LuaValue::Array(_), LuaValue::Map(_)) => Ordering::Less,
            (LuaValue::Array(v1), LuaValue::Array(v2)) => {
                let p1 = v1 as *const _ as usize;
                let p2 = v2 as *const _ as usize;
                p1.cmp(&p2)
            }
            (LuaValue::Null, LuaValue::Null) => Ordering::Equal,
            (LuaValue::Null, _) => Ordering::Less,
            (_, LuaValue::Null) => Ordering::Greater,
        })
    }
}
impl Ord for LuaValue {
    fn cmp(&self, other: &LuaValue) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}
impl PartialEq for LuaValue {
    fn eq(&self, other: &LuaValue) -> bool {
        match (self, other) {
            (LuaValue::Map(m1), LuaValue::Map(m2)) => {
                let p1 = m1 as *const _ as usize;
                let p2 = m2 as *const _ as usize;
                p1.eq(&p2)
            }
            (LuaValue::Array(v1), LuaValue::Array(v2)) => {
                let p1 = v1 as *const _ as usize;
                let p2 = v2 as *const _ as usize;
                p1.eq(&p2)
            }
            (LuaValue::String(s1), LuaValue::String(s2)) => s1.eq(s2),
            (LuaValue::Number(n1), LuaValue::Number(n2)) => n1.eq(n2),
            (LuaValue::Boolean(b1), LuaValue::Boolean(b2)) => b1.eq(b2),
            (LuaValue::Null, LuaValue::Null) => true,
            _ => false,
        }
    }
}
impl Eq for LuaValue {}

impl Hash for LuaMapKey {
    #[inline(always)]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}
impl PartialOrd for LuaMapKey {
    #[inline(always)]
    fn partial_cmp(&self, other: &LuaMapKey) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}
impl Ord for LuaMapKey {
    #[inline(always)]
    fn cmp(&self, other: &LuaMapKey) -> Ordering {
        self.0.cmp(&other.0)
    }
}
impl PartialEq for LuaMapKey {
    #[inline(always)]
    fn eq(&self, other: &LuaMapKey) -> bool {
        self.0.eq(&other.0)
    }
}
impl Eq for LuaMapKey {}

use std::borrow::Cow;
impl LuaMapKey {
    #[allow(dead_code)]
    fn to_string(&self) -> Cow<'_, str> {
        match self.0 {
            LuaValue::String(ref v) => Cow::from(v),
            LuaValue::Number(v) => Cow::from(v.to_string()),
            LuaValue::Boolean(v) => Cow::from(v.to_string()),
            LuaValue::Map(ref m) => Cow::from(format!("map at {:p}", m)),
            LuaValue::Array(ref m) => Cow::from(format!("array at {:p}", m)),
            LuaValue::Null => unsafe { core::hint::unreachable_unchecked() },
        }
    }
}

use std::fmt::{self, Debug};
impl Debug for LuaMapKey {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

#[cfg(feature = "serde")]
impl Serialize for LuaValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::{SerializeMap, SerializeSeq};

        match self {
            LuaValue::String(s) => serializer.serialize_str(s),
            LuaValue::Number(n) => serializer.serialize_f64(*n),
            LuaValue::Boolean(b) => serializer.serialize_bool(*b),
            LuaValue::Null => serializer.serialize_none(),
            LuaValue::Map(m) => {
                let mut map = serializer.serialize_map(Some(m.len()))?;
                for (k, v) in m {
                    map.serialize_entry(&LuaMapKey::to_string(k), v)?;
                }
                map.end()
            }
            LuaValue::Array(v) => {
                let mut seq = serializer.serialize_seq(Some(v.len()))?;
                for el in v {
                    seq.serialize_element(el)?;
                }
                seq.end()
            }
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for LuaValue {
    fn deserialize<D>(deserializer: D) -> Result<LuaValue, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LuaValueVisitor;

        impl<'de> Visitor<'de> for LuaValueVisitor {
            type Value = LuaValue;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a Lua value")
            }

            fn visit_none<E>(self) -> Result<LuaValue, E> {
                Ok(LuaValue::Null)
            }

            fn visit_unit<E>(self) -> Result<LuaValue, E> {
                Ok(LuaValue::Null)
            }

            fn visit_bool<E>(self, value: bool) -> Result<LuaValue, E> {
                Ok(LuaValue::Boolean(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<LuaValue, E>
            where
                E: de::Error,
            {
                let value_f64 = value as f64;
                if value_f64 as i64 == value {
                    Ok(LuaValue::Number(value_f64))
                } else {
                    Err(de::Error::custom("can't represent as f64"))
                }
            }

            fn visit_u64<E>(self, value: u64) -> Result<LuaValue, E>
            where
                E: de::Error,
            {
                let value_f64 = value as f64;
                if value_f64 as u64 == value {
                    Ok(LuaValue::Number(value_f64))
                } else {
                    Err(de::Error::custom("can't represent as f64"))
                }
            }

            fn visit_f64<E>(self, value: f64) -> Result<LuaValue, E> {
                Ok(LuaValue::Number(value))
            }

            fn visit_str<E>(self, value: &str) -> Result<LuaValue, E>
            where
                E: de::Error,
            {
                self.visit_string(String::from(value))
            }

            fn visit_string<E>(self, value: String) -> Result<LuaValue, E> {
                Ok(LuaValue::String(value))
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<LuaValue, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let mut result = Vec::with_capacity(seq.size_hint().unwrap_or(16));

                while let Some(element) = seq.next_element()? {
                    result.push(element);
                }

                Ok(LuaValue::Array(result))
            }

            fn visit_map<V>(self, mut map: V) -> Result<LuaValue, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut result = Map::with_capacity(map.size_hint().unwrap_or(16));

                while let Some(key) = map.next_key()? {
                    let key = LuaMapKey::from_value(match key {
                        LuaValue::String(s) => match s.parse::<i32>() {
                            Ok(n) => LuaValue::Number(n as f64),
                            Err(_) => LuaValue::String(s),
                        },
                        v => v,
                    })
                    .map_err(de::Error::custom)?;

                    let value = map.next_value()?;

                    result.insert(key, value);
                }

                Ok(LuaValue::Map(result))
            }
        }

        deserializer.deserialize_any(LuaValueVisitor)
    }
}
