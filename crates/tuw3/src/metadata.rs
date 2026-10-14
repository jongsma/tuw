// SPDX-License-Identifier: GPL-3.0-or-later

//! The metadata block (`SPEC.md`, *Metadata*): deterministic serialisation.

use serde_json::{Map, Value};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_metadata_is_an_empty_block() {
        assert_eq!(Metadata::default().to_json(), b"");
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
    fn empty_string_is_written() {
        let metadata = Metadata {
            name: Some(String::new()),
            ..Metadata::default()
        };
        assert_eq!(metadata.to_json(), br#"{"name":""}"#);
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
}
