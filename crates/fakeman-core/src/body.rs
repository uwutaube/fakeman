//! Builds a request body from fields and encodes it in the endpoint's body format.

use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::generators::{GeneratorError, generate};
use crate::model::{BodyFormat, Field, Header};
use crate::rng::GenContext;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BodyError {
    #[error("a field has no name")]
    EmptyName,
    #[error("field name `{0}` has an empty part between dots")]
    EmptySegment(String),
    #[error("field name `{0}` is used more than once")]
    DuplicateName(String),
    #[error("field `{child}` would nest inside `{parent}`, which is also a field")]
    NameConflict { parent: String, child: String },
    #[error("field `{field}`: {source}")]
    Generator {
        field: String,
        #[source]
        source: GeneratorError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    #[error("a form body must be a JSON object")]
    NotAnObject,
    #[error("form value `{0}` must be a string, number, boolean or null")]
    NotFlat(String),
}

/// Checks the names of the enabled fields without generating anything.
///
/// Names must be non-empty and unique. In JSON, dotted names nest, so every part between dots
/// must be non-empty and a name may not also be the parent of another (`a` and `a.b`). In
/// form-urlencoded, names are literal keys, so `a` and `a.b` can coexist.
pub fn check_names(fields: &[Field], format: BodyFormat) -> Result<(), BodyError> {
    let mut seen = HashSet::new();
    for field in fields.iter().filter(|field| field.enabled) {
        let name = field.name.as_str();
        if name.is_empty() {
            return Err(BodyError::EmptyName);
        }
        if format == BodyFormat::Json && name.split('.').any(str::is_empty) {
            return Err(BodyError::EmptySegment(name.into()));
        }
        if !seen.insert(name) {
            return Err(BodyError::DuplicateName(name.into()));
        }
    }
    if format == BodyFormat::Json {
        for name in fields
            .iter()
            .filter(|field| field.enabled)
            .map(|field| field.name.as_str())
        {
            if let Some((end, _)) = name
                .match_indices('.')
                .find(|&(end, _)| seen.contains(&name[..end]))
            {
                return Err(BodyError::NameConflict {
                    parent: name[..end].into(),
                    child: name.into(),
                });
            }
        }
    }
    Ok(())
}

/// Generates a fresh value for every enabled field and assembles them into a JSON object, in
/// field order. In JSON format `address.city` becomes `{"address": {"city": ...}}`; in
/// form-urlencoded format it stays the literal key `address.city`.
pub fn build_body(
    fields: &[Field],
    format: BodyFormat,
    ctx: &mut GenContext,
) -> Result<Value, BodyError> {
    check_names(fields, format)?;
    let mut body = Map::new();
    for field in fields.iter().filter(|field| field.enabled) {
        let value = generate(&field.generator, ctx).map_err(|source| BodyError::Generator {
            field: field.name.clone(),
            source,
        })?;
        match format {
            BodyFormat::Json => insert_nested(&mut body, &field.name, value),
            BodyFormat::FormUrlEncoded => {
                body.insert(field.name.clone(), value);
            }
        }
    }
    Ok(Value::Object(body))
}

/// Inserts `value` at a dotted path. [`check_names`] guarantees no part of the path is
/// already a leaf.
fn insert_nested(body: &mut Map<String, Value>, name: &str, value: Value) {
    let mut parts = name.split('.').peekable();
    let mut current = body;
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            current.insert(part.into(), value);
            return;
        }
        let child = current
            .entry(part)
            .or_insert_with(|| Value::Object(Map::new()));
        let Value::Object(child) = child else {
            return;
        };
        current = child;
    }
}

/// Encodes a body produced by [`build_body`].
///
/// JSON keeps value types. Form-urlencoded writes `key=value` pairs with keys as written,
/// numbers and booleans as their JSON text, and `null` as an empty value.
pub fn encode_body(body: &Value, format: BodyFormat) -> Result<String, EncodeError> {
    match format {
        BodyFormat::Json => Ok(body.to_string()),
        BodyFormat::FormUrlEncoded => {
            let Value::Object(map) = body else {
                return Err(EncodeError::NotAnObject);
            };
            let mut form = form_urlencoded::Serializer::new(String::new());
            for (key, value) in map {
                let text = match value {
                    Value::String(text) => text.clone(),
                    Value::Number(number) => number.to_string(),
                    Value::Bool(flag) => flag.to_string(),
                    Value::Null => String::new(),
                    Value::Array(_) | Value::Object(_) => {
                        return Err(EncodeError::NotFlat(key.clone()));
                    }
                };
                form.append_pair(key, &text);
            }
            Ok(form.finish())
        }
    }
}

/// The `Content-Type` to send: the first enabled, non-empty `Content-Type` header the user set
/// (matched case-insensitively), otherwise the default for the body format.
pub fn content_type(headers: &[Header], format: BodyFormat) -> &str {
    headers
        .iter()
        .filter(|header| header.enabled && header.key.trim().eq_ignore_ascii_case("content-type"))
        .map(|header| header.value.trim())
        .find(|value| !value.is_empty())
        .unwrap_or(format.default_content_type())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generators::catalog;
    use crate::model::GeneratorRef;
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    fn field(name: &str, key: &str, params: Value) -> Field {
        Field {
            name: name.into(),
            generator: GeneratorRef::with_params(key, params),
            enabled: true,
        }
    }

    fn fixed(name: &str, value: Value) -> Field {
        let key = match value {
            Value::String(_) => "static.string",
            Value::Number(_) => "static.number",
            Value::Bool(_) => "static.boolean",
            _ => "static.null",
        };
        field(name, key, json!({ "value": value }))
    }

    fn header(key: &str, value: &str, enabled: bool) -> Header {
        Header {
            key: key.into(),
            value: value.into(),
            enabled,
        }
    }

    fn every_generator() -> Vec<Field> {
        catalog()
            .iter()
            .map(|def| field(def.key, def.key, json!({})))
            .collect()
    }

    fn ctx(seed: u64) -> GenContext {
        GenContext::seeded(seed).at(Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap())
    }

    #[test]
    fn dotted_names_nest_in_json() {
        let fields = [
            fixed("name", json!("Lena")),
            fixed("address.city", json!("Berlin")),
            fixed("address.geo.lat", json!(52.5)),
            fixed("address.zip", json!(10115)),
        ];
        let body = build_body(&fields, BodyFormat::Json, &mut ctx(1)).unwrap();
        assert_eq!(
            body,
            json!({
                "name": "Lena",
                "address": { "city": "Berlin", "geo": { "lat": 52.5 }, "zip": 10115 }
            })
        );
        assert_eq!(
            encode_body(&body, BodyFormat::Json).unwrap(),
            r#"{"name":"Lena","address":{"city":"Berlin","geo":{"lat":52.5},"zip":10115}}"#
        );
    }

    #[test]
    fn dotted_names_stay_flat_in_form() {
        let fields = [
            fixed("address.city", json!("Berlin")),
            fixed("address", json!("x")),
        ];
        let body = build_body(&fields, BodyFormat::FormUrlEncoded, &mut ctx(1)).unwrap();
        assert_eq!(body, json!({ "address.city": "Berlin", "address": "x" }));
        assert_eq!(
            encode_body(&body, BodyFormat::FormUrlEncoded).unwrap(),
            "address.city=Berlin&address=x"
        );
    }

    #[test]
    fn disabled_fields_are_skipped_and_ignored_by_name_checks() {
        let mut off = fixed("a", json!(1));
        off.enabled = false;
        let mut off_unknown = field("b", "no.suchKey", json!({}));
        off_unknown.enabled = false;
        let fields = [off, off_unknown, fixed("a.b", json!(2))];
        let body = build_body(&fields, BodyFormat::Json, &mut ctx(1)).unwrap();
        assert_eq!(body, json!({ "a": { "b": 2 } }));
    }

    #[test]
    fn duplicate_names_are_rejected_in_both_formats() {
        let fields = [fixed("email", json!("a")), fixed("email", json!("b"))];
        for format in [BodyFormat::Json, BodyFormat::FormUrlEncoded] {
            assert_eq!(
                build_body(&fields, format, &mut ctx(1)),
                Err(BodyError::DuplicateName("email".into()))
            );
        }
    }

    #[test]
    fn parent_and_child_names_conflict_in_json_in_either_order() {
        let expected = Err(BodyError::NameConflict {
            parent: "a".into(),
            child: "a.b.c".into(),
        });
        let fields = [fixed("a", json!(1)), fixed("a.b.c", json!(2))];
        assert_eq!(check_names(&fields, BodyFormat::Json), expected);
        let fields = [fixed("a.b.c", json!(2)), fixed("a", json!(1))];
        assert_eq!(check_names(&fields, BodyFormat::Json), expected);
        let fields = [fixed("a.b", json!(1)), fixed("a.b.c", json!(2))];
        assert_eq!(
            check_names(&fields, BodyFormat::Json),
            Err(BodyError::NameConflict {
                parent: "a.b".into(),
                child: "a.b.c".into()
            })
        );
        let siblings = [
            fixed("a.b", json!(1)),
            fixed("a.c", json!(2)),
            fixed("ab", json!(3)),
        ];
        assert_eq!(check_names(&siblings, BodyFormat::Json), Ok(()));
    }

    #[test]
    fn empty_names_and_segments_are_rejected() {
        assert_eq!(
            check_names(&[fixed("", json!(1))], BodyFormat::FormUrlEncoded),
            Err(BodyError::EmptyName)
        );
        for name in [".a", "a.", "a..b"] {
            assert_eq!(
                check_names(&[fixed(name, json!(1))], BodyFormat::Json),
                Err(BodyError::EmptySegment(name.into()))
            );
            assert_eq!(
                check_names(&[fixed(name, json!(1))], BodyFormat::FormUrlEncoded),
                Ok(())
            );
        }
    }

    #[test]
    fn generator_errors_name_the_field() {
        let fields = [field("age", "number.int", json!({ "min": 5, "max": 1 }))];
        let err = build_body(&fields, BodyFormat::Json, &mut ctx(1)).unwrap_err();
        assert!(matches!(&err, BodyError::Generator { field, .. } if field == "age"));
        let fields = [field("x", "no.suchKey", json!({}))];
        assert_eq!(
            build_body(&fields, BodyFormat::Json, &mut ctx(1)),
            Err(BodyError::Generator {
                field: "x".into(),
                source: GeneratorError::UnknownKey("no.suchKey".into())
            })
        );
    }

    #[test]
    fn same_seed_gives_same_body() {
        let fields = every_generator();
        for format in [BodyFormat::Json, BodyFormat::FormUrlEncoded] {
            let first = build_body(&fields, format, &mut ctx(42)).unwrap();
            let second = build_body(&fields, format, &mut ctx(42)).unwrap();
            assert_eq!(first, second);
            assert_ne!(first, build_body(&fields, format, &mut ctx(43)).unwrap());
        }
    }

    #[test]
    fn unseeded_builds_differ() {
        let fields = every_generator();
        let first = build_body(&fields, BodyFormat::Json, &mut GenContext::new()).unwrap();
        let second = build_body(&fields, BodyFormat::Json, &mut GenContext::new()).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn one_context_gives_fresh_values_per_build() {
        let fields = [field("id", "datatype.uuid", json!({}))];
        let mut ctx = ctx(7);
        let first = build_body(&fields, BodyFormat::Json, &mut ctx).unwrap();
        let second = build_body(&fields, BodyFormat::Json, &mut ctx).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn form_values_are_stringified_and_escaped() {
        let body = json!({
            "name": "Lena Müller",
            "q": "a&b=c+d/e?f#%",
            "tags[x]": "1",
            "age": 42,
            "price": 9.5,
            "active": true,
            "deleted": false,
            "nickname": null,
            "": "empty key",
        });
        assert_eq!(
            encode_body(&body, BodyFormat::FormUrlEncoded).unwrap(),
            "name=Lena+M%C3%BCller&q=a%26b%3Dc%2Bd%2Fe%3Ff%23%25&tags%5Bx%5D=1&age=42\
             &price=9.5&active=true&deleted=false&nickname=&=empty+key"
        );
    }

    #[test]
    fn form_encoding_round_trips_through_a_decoder() {
        let fields = every_generator();
        let body = build_body(&fields, BodyFormat::FormUrlEncoded, &mut ctx(3)).unwrap();
        let encoded = encode_body(&body, BodyFormat::FormUrlEncoded).unwrap();
        let decoded: Vec<(String, String)> = form_urlencoded::parse(encoded.as_bytes())
            .into_owned()
            .collect();
        let Value::Object(map) = &body else {
            panic!("form body is an object")
        };
        assert_eq!(decoded.len(), map.len());
        for ((key, text), (expected_key, value)) in decoded.iter().zip(map) {
            assert_eq!(key, expected_key);
            let expected = match value {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                other => other.to_string(),
            };
            assert_eq!(text, &expected);
        }
    }

    #[test]
    fn form_encoding_rejects_nested_values() {
        assert_eq!(
            encode_body(&json!({ "a": { "b": 1 } }), BodyFormat::FormUrlEncoded),
            Err(EncodeError::NotFlat("a".into()))
        );
        assert_eq!(
            encode_body(&json!({ "a": [1] }), BodyFormat::FormUrlEncoded),
            Err(EncodeError::NotFlat("a".into()))
        );
        assert_eq!(
            encode_body(&json!("text"), BodyFormat::FormUrlEncoded),
            Err(EncodeError::NotAnObject)
        );
    }

    #[test]
    fn json_encoding_keeps_types() {
        let body = json!({ "n": 1, "f": 1.5, "b": true, "z": null, "s": "a\"b" });
        assert_eq!(
            encode_body(&body, BodyFormat::Json).unwrap(),
            r#"{"n":1,"f":1.5,"b":true,"z":null,"s":"a\"b"}"#
        );
    }

    #[test]
    fn content_type_defaults_by_format() {
        assert_eq!(content_type(&[], BodyFormat::Json), "application/json");
        assert_eq!(
            content_type(
                &[header("Authorization", "x", true)],
                BodyFormat::FormUrlEncoded
            ),
            "application/x-www-form-urlencoded"
        );
    }

    #[test]
    fn user_content_type_header_overrides_default() {
        let headers = [
            header("Content-Type", "text/plain", false),
            header("content-type", "  ", true),
            header(" CONTENT-TYPE ", "application/vnd.api+json", true),
            header("Content-Type", "application/other", true),
        ];
        assert_eq!(
            content_type(&headers, BodyFormat::Json),
            "application/vnd.api+json"
        );
        assert_eq!(
            content_type(&headers[..2], BodyFormat::Json),
            "application/json"
        );
    }
}
