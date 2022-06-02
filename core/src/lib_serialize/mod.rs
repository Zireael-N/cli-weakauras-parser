pub mod deserialization;
pub mod serialization;
pub(crate) mod type_tag;

pub use deserialization::Deserializer;
pub use serialization::Serializer;
pub(crate) use type_tag::{EmbeddedTypeTag, TypeTag};

#[allow(dead_code)]
pub(crate) const MAJOR: &str = "LibSerialize";
#[allow(dead_code)]
pub(crate) const MINOR: u8 = 1;
