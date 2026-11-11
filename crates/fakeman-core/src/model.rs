//! Domain types shared by the store, the runner and the UI.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EndpointId(pub i64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Endpoint {
    pub id: EndpointId,
    pub name: String,
    pub method: Method,
    pub url: String,
    pub body_format: BodyFormat,
    pub headers: Vec<Header>,
    pub fields: Vec<Field>,
    pub run: RunSettings,
}

/// HTTP method. Only POST exists in v1; the type is stored so more can be added later.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Method {
    #[default]
    #[serde(rename = "POST")]
    Post,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Post => "POST",
        }
    }
}

/// How the generated body is encoded. Serialised as `json` / `form`, matching the database.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyFormat {
    #[default]
    Json,
    #[serde(rename = "form")]
    FormUrlEncoded,
}

impl BodyFormat {
    pub fn default_content_type(self) -> &'static str {
        match self {
            BodyFormat::Json => "application/json",
            BodyFormat::FormUrlEncoded => "application/x-www-form-urlencoded",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub generator: GeneratorRef,
    pub enabled: bool,
}

/// A catalog key plus its parameters, as stored in the database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratorRef {
    pub key: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

impl GeneratorRef {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            params: serde_json::Value::Object(Default::default()),
        }
    }

    pub fn with_params(key: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            key: key.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSettings {
    pub count: u32,
    pub delay_ms: u64,
}

impl Default for RunSettings {
    fn default() -> Self {
        Self {
            count: 1,
            delay_ms: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn enums_serialise_to_database_values() {
        assert_eq!(serde_json::to_value(Method::Post).unwrap(), json!("POST"));
        assert_eq!(
            serde_json::to_value(BodyFormat::Json).unwrap(),
            json!("json")
        );
        assert_eq!(
            serde_json::to_value(BodyFormat::FormUrlEncoded).unwrap(),
            json!("form")
        );
    }

    #[test]
    fn endpoint_round_trips_through_serde() {
        let endpoint = Endpoint {
            id: EndpointId(7),
            name: "Create user".into(),
            method: Method::Post,
            url: "https://api.example.com/users".into(),
            body_format: BodyFormat::FormUrlEncoded,
            headers: vec![Header {
                key: "Authorization".into(),
                value: "Bearer x".into(),
                enabled: true,
            }],
            fields: vec![Field {
                name: "address.city".into(),
                generator: GeneratorRef::with_params("number.int", json!({"min": 1, "max": 5})),
                enabled: false,
            }],
            run: RunSettings {
                count: 3,
                delay_ms: 250,
            },
        };
        let text = serde_json::to_string(&endpoint).unwrap();
        assert_eq!(serde_json::from_str::<Endpoint>(&text).unwrap(), endpoint);
    }

    #[test]
    fn run_settings_default_to_one_send_without_delay() {
        assert_eq!(
            RunSettings::default(),
            RunSettings {
                count: 1,
                delay_ms: 0
            }
        );
    }
}
