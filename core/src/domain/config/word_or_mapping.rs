//! A key written as one word or as a mapping: `production: view`, or
//! `production: {level: deploy, exec: true}`.

use std::marker::PhantomData;

use serde::de::value::MapAccessDeserializer;
use serde::de::{Error, IntoDeserializer, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

/// Reads a word as `W` and a mapping as `M`, so the keys of the mapping
/// are read where the deserializer sees them.
pub(crate) fn word_or_mapping<'de, D, W, M, T>(
    deserializer: D,
    expecting: &'static str,
    word: fn(W) -> T,
    mapping: fn(M) -> T,
) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    W: Deserialize<'de>,
    M: Deserialize<'de>,
{
    deserializer.deserialize_any(WordOrMapping {
        expecting,
        word,
        mapping,
        read: PhantomData,
    })
}

struct WordOrMapping<W, M, T> {
    expecting: &'static str,
    word: fn(W) -> T,
    mapping: fn(M) -> T,
    read: PhantomData<(W, M)>,
}

impl<'de, W, M, T> Visitor<'de> for WordOrMapping<W, M, T>
where
    W: Deserialize<'de>,
    M: Deserialize<'de>,
{
    type Value = T;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.expecting)
    }

    fn visit_str<E: Error>(self, value: &str) -> Result<T, E> {
        W::deserialize(value.into_deserializer()).map(self.word)
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
        M::deserialize(MapAccessDeserializer::new(map)).map(self.mapping)
    }
}
