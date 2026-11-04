//! Decode our JSON-only IPC enums without Serde's per-variant Content buffer.
//!
//! The wire format remains `{ "op": "name", ...fields }`. A remote derive on
//! the enum supplies the normal field/default/type checks; this adapter feeds
//! it an EnumAccess backed by the same JSON map for every variant.
#[cfg(test)]
mod tests;

use serde::{
    Deserialize, Deserializer,
    de::{
        DeserializeSeed, EnumAccess, Error, IntoDeserializer, VariantAccess, Visitor,
        value::EnumAccessDeserializer,
    },
};
use serde_json::{Map, Value};

pub fn deserialize<'de, D: Deserializer<'de>, T>(
    deserializer: D,
    tag: &'static str,
    decode: impl FnOnce(EnumAccessDeserializer<TaggedObject>) -> Result<T, serde_json::Error>,
) -> Result<T, D::Error> {
    let mut fields = Map::<String, Value>::deserialize(deserializer)?;
    let variant = fields
        .remove(tag)
        .ok_or_else(|| D::Error::missing_field(tag))?;
    let variant = String::deserialize(variant).map_err(D::Error::custom)?;
    decode(EnumAccessDeserializer::new(TaggedObject {
        variant,
        fields,
    }))
    .map_err(D::Error::custom)
}

pub struct TaggedObject {
    variant: String,
    fields: Map<String, Value>,
}

pub struct TaggedFields(Map<String, Value>);

impl<'de> EnumAccess<'de> for TaggedObject {
    type Error = serde_json::Error;
    type Variant = TaggedFields;

    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Self::Error> {
        let variant = seed.deserialize(self.variant.into_deserializer())?;
        Ok((variant, TaggedFields(self.fields)))
    }
}

impl<'de> VariantAccess<'de> for TaggedFields {
    type Error = serde_json::Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        // Internally tagged unit variants ignore unknown fields, just as the
        // struct variants do unless their derive uses deny_unknown_fields.
        Ok(())
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<T::Value, Self::Error> {
        seed.deserialize(Value::Object(self.0))
    }

    fn tuple_variant<V: Visitor<'de>>(
        self,
        _len: usize,
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        Err(Self::Error::custom(
            "tagged JSON commands cannot contain tuple variants",
        ))
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        Value::Object(self.0).deserialize_map(visitor)
    }
}
