//! Conversion between the store's plain records and the core domain types.

use std::borrow::Cow;

use fakeman_core::{
    BodyFormat, Endpoint, EndpointId, Field, GeneratorRef, Header, Method, RunSettings,
};
use fakeman_store::{BodyFormat as StoredFormat, EndpointRecord, FieldRecord, HeaderRecord};

/// Shown in place of a generator key the catalog does not know, for example
/// one saved by a newer version of fakeman.
pub const UNKNOWN_GENERATOR: &str = "Unknown generator";

pub fn endpoint_from_record(record: EndpointRecord) -> Endpoint {
    Endpoint {
        id: EndpointId(record.id),
        name: record.name,
        method: Method::Post,
        url: record.url,
        body_format: match record.body_format {
            StoredFormat::Json => BodyFormat::Json,
            StoredFormat::Form => BodyFormat::FormUrlEncoded,
        },
        headers: record
            .headers
            .into_iter()
            .map(|h| Header {
                key: h.key,
                value: h.value,
                enabled: h.enabled,
            })
            .collect(),
        fields: record
            .fields
            .into_iter()
            .map(|f| Field {
                name: f.name,
                generator: GeneratorRef::with_params(f.generator_key, f.params),
                enabled: f.enabled,
            })
            .collect(),
        run: RunSettings {
            count: record.run_count,
            delay_ms: record.delay_ms,
        },
    }
}

/// Builds the record that `Store::save` writes. `save` ignores `sort_order`
/// and the timestamps, so they are left empty here.
pub fn record_from_endpoint(endpoint: &Endpoint) -> EndpointRecord {
    EndpointRecord {
        id: endpoint.id.0,
        name: endpoint.name.clone(),
        method: endpoint.method.as_str().to_owned(),
        url: endpoint.url.clone(),
        body_format: match endpoint.body_format {
            BodyFormat::Json => StoredFormat::Json,
            BodyFormat::FormUrlEncoded => StoredFormat::Form,
        },
        run_count: endpoint.run.count,
        delay_ms: endpoint.run.delay_ms,
        sort_order: 0,
        created_at: String::new(),
        updated_at: String::new(),
        fields: endpoint
            .fields
            .iter()
            .map(|f| FieldRecord {
                name: f.name.clone(),
                generator_key: f.generator.key.clone(),
                params: f.generator.params.clone(),
                enabled: f.enabled,
            })
            .collect(),
        headers: endpoint
            .headers
            .iter()
            .map(|h| HeaderRecord {
                key: h.key.clone(),
                value: h.value.clone(),
                enabled: h.enabled,
            })
            .collect(),
    }
}

/// The text the field table shows for a generator: its catalog key, or
/// "Unknown generator (<key>)" when the catalog has no such key.
pub fn generator_display(generator: &GeneratorRef) -> Cow<'_, str> {
    match fakeman_core::lookup(&generator.key) {
        Ok(def) => Cow::Borrowed(def.key),
        Err(_) => Cow::Owned(format!("{UNKNOWN_GENERATOR} ({})", generator.key)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn record() -> EndpointRecord {
        EndpointRecord {
            id: 7,
            name: "Create user".into(),
            method: "POST".into(),
            url: "https://api.example.com/users".into(),
            body_format: StoredFormat::Form,
            run_count: 3,
            delay_ms: 250,
            sort_order: 0,
            created_at: String::new(),
            updated_at: String::new(),
            fields: vec![
                FieldRecord {
                    name: "age".into(),
                    generator_key: "number.int".into(),
                    params: json!({"min": 1, "max": 5}),
                    enabled: false,
                },
                FieldRecord {
                    name: "future".into(),
                    generator_key: "future.thing".into(),
                    params: json!({"x": true}),
                    enabled: true,
                },
            ],
            headers: vec![HeaderRecord {
                key: "Authorization".into(),
                value: "Bearer x".into(),
                enabled: true,
            }],
        }
    }

    #[test]
    fn record_maps_to_endpoint() {
        let endpoint = endpoint_from_record(record());
        assert_eq!(endpoint.id, EndpointId(7));
        assert_eq!(endpoint.method, Method::Post);
        assert_eq!(endpoint.body_format, BodyFormat::FormUrlEncoded);
        assert_eq!(
            endpoint.run,
            RunSettings {
                count: 3,
                delay_ms: 250
            }
        );
        assert_eq!(endpoint.fields[0].generator.key, "number.int");
        assert_eq!(
            endpoint.fields[0].generator.params,
            json!({"min": 1, "max": 5})
        );
        assert!(!endpoint.fields[0].enabled);
        assert_eq!(endpoint.headers[0].value, "Bearer x");
    }

    #[test]
    fn endpoint_round_trips_to_the_same_record() {
        let original = record();
        assert_eq!(
            record_from_endpoint(&endpoint_from_record(original.clone())),
            original
        );
    }

    #[test]
    fn json_format_maps_both_ways() {
        let mut stored = record();
        stored.body_format = StoredFormat::Json;
        let endpoint = endpoint_from_record(stored);
        assert_eq!(endpoint.body_format, BodyFormat::Json);
        assert_eq!(
            record_from_endpoint(&endpoint).body_format,
            StoredFormat::Json
        );
    }

    #[test]
    fn unknown_generator_loads_and_is_shown_as_unknown() {
        let endpoint = endpoint_from_record(record());
        let unknown = &endpoint.fields[1].generator;
        assert_eq!(unknown.key, "future.thing");
        assert_eq!(
            generator_display(unknown),
            "Unknown generator (future.thing)"
        );
        assert_eq!(
            generator_display(&endpoint.fields[0].generator),
            "number.int"
        );
    }
}
