// SPDX-License-Identifier: GPL-3.0-or-later

//! The metadata block (`SPEC.md`, *Metadata*): parsing with duplicate-key and depth checks, and
//! deterministic serialisation.

use std::cell::Cell;
use std::fmt;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

use crate::error::MetadataError;
use crate::limits::MAX_META_DEPTH;

/// Descriptive information about a table. Every field is optional.
///
/// Unknown keys found in a file are kept privately and written back by
/// [`write`](crate::write), so tools can add their own keys without losing them in TUWE.
///
/// Because of that private part, create metadata with [`Metadata::default`] and then set
/// fields:
///
/// ```
/// let mut metadata = tuw3::Metadata::default();
/// metadata.name = Some("Vowel cube".to_owned());
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metadata {
    /// Name of the table.
    pub name: Option<String>,
    /// Author(s).
    pub author: Option<String>,
    /// Licence of the table, preferably an SPDX identifier such as `CC-BY-4.0`.
    pub license: Option<String>,
    /// What the Y axis represents.
    pub y_label: Option<String>,
    /// What the Z axis represents.
    pub z_label: Option<String>,
    /// Free text.
    pub description: Option<String>,
    /// Keys this crate does not know, preserved for writing.
    extra: Map<String, Value>,
}

impl Metadata {
    /// The known fields with their keys, in the order of the spec's table.
    fn known_fields(&self) -> [(&'static str, &Option<String>); 6] {
        [
            ("name", &self.name),
            ("author", &self.author),
            ("license", &self.license),
            ("y_label", &self.y_label),
            ("z_label", &self.z_label),
            ("description", &self.description),
        ]
    }

    /// Parses a metadata block. An empty block means "no metadata".
    pub(crate) fn from_json(bytes: &[u8]) -> Result<Metadata, MetadataError> {
        if bytes.is_empty() {
            return Ok(Metadata::default());
        }
        let text = std::str::from_utf8(bytes).map_err(|_| MetadataError::NotUtf8)?;
        let Value::Object(mut object) = parse_checked(text)? else {
            return Err(MetadataError::NotAnObject);
        };

        // Take each known field out of the object; whatever is left is unknown.
        let mut take = |key: &str| match object.remove(key) {
            None => Ok(None),
            Some(Value::String(text)) => Ok(Some(text)),
            // `null`, numbers, arrays, ...: a known field must be a string when present.
            Some(_) => Err(MetadataError::WrongType {
                key: key.to_owned(),
            }),
        };
        let name = take("name")?;
        let author = take("author")?;
        let license = take("license")?;
        let y_label = take("y_label")?;
        let z_label = take("z_label")?;
        let description = take("description")?;

        Ok(Metadata {
            name,
            author,
            license,
            y_label,
            z_label,
            description,
            extra: object,
        })
    }

    /// Serialises to compact JSON: known fields in spec order (`None` left out), then unknown
    /// keys sorted, also inside nested objects. Empty metadata gives an empty block.
    pub(crate) fn to_json(&self) -> Vec<u8> {
        let present: Vec<(&str, &String)> = self
            .known_fields()
            .into_iter()
            .filter_map(|(key, value)| value.as_ref().map(|text| (key, text)))
            .collect();
        if present.is_empty() && self.extra.is_empty() {
            return Vec::new();
        }

        let mut out = Vec::new();
        out.push(b'{');
        let mut first = true;
        for (key, text) in present {
            write_entry(&mut out, &mut first, key, &Value::String(text.clone()));
        }
        for (key, value) in sorted_entries(&self.extra) {
            write_entry(&mut out, &mut first, key, value);
        }
        out.push(b'}');
        out
    }
}

/// Writes `"key":value`, preceded by a comma unless it is the first entry.
fn write_entry(out: &mut Vec<u8>, first: &mut bool, key: &str, value: &Value) {
    if !*first {
        out.push(b',');
    }
    *first = false;
    write_scalar(out, &Value::String(key.to_owned()));
    out.push(b':');
    write_sorted(out, value);
}

/// Writes any JSON value with the keys of every object sorted. This does not rely on how
/// `serde_json::Map` orders keys, which changes if any crate enables its `preserve_order`
/// feature.
fn write_sorted(out: &mut Vec<u8>, value: &Value) {
    match value {
        Value::Object(object) => {
            out.push(b'{');
            let mut first = true;
            for (key, inner) in sorted_entries(object) {
                write_entry(out, &mut first, key, inner);
            }
            out.push(b'}');
        }
        Value::Array(items) => {
            out.push(b'[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                write_sorted(out, item);
            }
            out.push(b']');
        }
        scalar => write_scalar(out, scalar),
    }
}

/// Writes a string, number, bool or null.
fn write_scalar(out: &mut Vec<u8>, value: &Value) {
    // Serialising a `Value` into a `Vec` has no I/O that can fail, and a `Value` holds no
    // non-finite numbers or non-string keys, so serde_json cannot return an error here.
    serde_json::to_writer(out, value).expect("serialising a JSON scalar cannot fail");
}

/// The entries of an object, sorted by key.
fn sorted_entries(object: &Map<String, Value>) -> Vec<(&str, &Value)> {
    let mut entries: Vec<(&str, &Value)> = object
        .iter()
        .map(|(key, value)| (key.as_str(), value))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    entries
}

/// Parses JSON text into a `Value`, rejecting duplicate keys and nesting deeper than
/// [`MAX_META_DEPTH`] (`SPEC.md`, *Metadata*).
///
/// `serde_json` on its own silently keeps the last of two equal keys, so the parse goes
/// through [`ValueSeed`], which checks each object as it is built. serde's error type cannot
/// carry our own error, so the problem found is passed out through a `Cell`.
fn parse_checked(text: &str) -> Result<Value, MetadataError> {
    let problem = Cell::new(None);
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let parsed = ValueSeed {
        problem: &problem,
        depth: 1,
    }
    .deserialize(&mut deserializer)
    .and_then(|value| deserializer.end().map(|()| value));
    match (parsed, problem.take()) {
        (_, Some(error)) => Err(error),
        (Ok(value), None) => Ok(value),
        (Err(error), None) => Err(MetadataError::InvalidJson(error.to_string())),
    }
}

/// Deserialises one JSON value of any type, recording the first duplicate key or
/// too-deep nesting it meets.
#[derive(Clone, Copy)]
struct ValueSeed<'a> {
    problem: &'a Cell<Option<MetadataError>>,
    /// Nesting level of the value: 1 for the top-level object.
    depth: u32,
}

impl ValueSeed<'_> {
    /// The seed for the elements of an array or the values of an object.
    fn inner(self) -> Self {
        ValueSeed {
            depth: self.depth + 1,
            ..self
        }
    }

    /// Fails if an array or object at this level is nested too deeply. Checking here, before
    /// serde_json's own limit of 128 is reached, makes the limit the spec's, not the parser's.
    fn check_depth<E: de::Error>(self) -> Result<(), E> {
        if self.depth > MAX_META_DEPTH {
            self.problem.set(Some(MetadataError::NestedTooDeeply));
            return Err(E::custom("nested too deeply"));
        }
        Ok(())
    }
}

impl<'de> DeserializeSeed<'de> for ValueSeed<'_> {
    type Value = Value;

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ValueSeed<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
        Ok(Value::from(value))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
        Ok(Value::from(value))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        // JSON text cannot express NaN or infinity, so this only fails on a parser bug.
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("number is not finite"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        self.check_depth()?;
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(self.inner())? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        self.check_depth()?;
        let mut object = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if object.contains_key(&key) {
                self.problem.set(Some(MetadataError::DuplicateKey { key }));
                return Err(de::Error::custom("duplicate key"));
            }
            let value = map.next_value_seed(self.inner())?;
            object.insert(key, value);
        }
        Ok(Value::Object(object))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_block_is_empty_metadata() {
        assert_eq!(Metadata::from_json(b""), Ok(Metadata::default()));
        assert_eq!(Metadata::default().to_json(), b"");
    }

    #[test]
    fn known_fields_are_written_in_spec_order_before_sorted_unknown_keys() {
        let metadata =
            Metadata::from_json(br#"{"zz":1,"description":"d","aa":{"b":2,"a":1},"name":"n"}"#)
                .unwrap();
        assert_eq!(
            String::from_utf8(metadata.to_json()).unwrap(),
            r#"{"name":"n","description":"d","aa":{"a":1,"b":2},"zz":1}"#
        );
    }

    #[test]
    fn empty_string_is_written() {
        let metadata = Metadata {
            name: Some(String::new()),
            ..Metadata::default()
        };
        assert_eq!(metadata.to_json(), br#"{"name":""}"#);
    }

    #[test]
    fn known_fields_are_written_in_spec_order() {
        let metadata = Metadata {
            description: Some("d".to_owned()),
            name: Some("n".to_owned()),
            z_label: Some("z".to_owned()),
            ..Metadata::default()
        };
        assert_eq!(
            String::from_utf8(metadata.to_json()).unwrap(),
            r#"{"name":"n","z_label":"z","description":"d"}"#
        );
    }

    #[test]
    fn characters_are_escaped() {
        let metadata = Metadata {
            name: Some("\"\\\n\u{0}é".to_owned()),
            ..Metadata::default()
        };
        assert_eq!(
            String::from_utf8(metadata.to_json()).unwrap(),
            r#"{"name":"\"\\\n\u0000é"}"#
        );
    }

    #[test]
    fn duplicate_keys_are_rejected_at_any_depth() {
        let top = Metadata::from_json(br#"{"license":"CC0-1.0","license":"x"}"#);
        assert_eq!(
            top,
            Err(MetadataError::DuplicateKey {
                key: "license".to_owned()
            })
        );
        // The second key is spelled with a JSON escape (`\u0061` is "a").
        let escaped = Metadata::from_json(br#"{"a":1,"\u0061":2}"#);
        assert_eq!(
            escaped,
            Err(MetadataError::DuplicateKey {
                key: "a".to_owned()
            })
        );
        let nested = Metadata::from_json(br#"{"tool":[{"a":1,"a":2}]}"#);
        assert_eq!(
            nested,
            Err(MetadataError::DuplicateKey {
                key: "a".to_owned()
            })
        );
    }

    #[test]
    fn known_field_must_be_a_string() {
        for json in [r#"{"name":null}"#, r#"{"name":5}"#, r#"{"name":["a"]}"#] {
            assert_eq!(
                Metadata::from_json(json.as_bytes()),
                Err(MetadataError::WrongType {
                    key: "name".to_owned()
                }),
                "{json}"
            );
        }
    }

    #[test]
    fn invalid_blocks_are_rejected() {
        assert_eq!(
            Metadata::from_json(&[0xff, 0xfe]),
            Err(MetadataError::NotUtf8)
        );
        assert!(matches!(
            Metadata::from_json(b"{\"name\":"),
            Err(MetadataError::InvalidJson(_))
        ));
        assert!(matches!(
            Metadata::from_json(b"{} {}"),
            Err(MetadataError::InvalidJson(_))
        ));
        assert_eq!(
            Metadata::from_json(b"[1,2]"),
            Err(MetadataError::NotAnObject)
        );
    }
}
