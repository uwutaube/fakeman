use std::fmt;
use std::str::FromStr;

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};

/// Primary key of an endpoint row.
pub type EndpointId = i64;

/// How an endpoint's body is encoded, stored as `json` or `form`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BodyFormat {
    #[default]
    Json,
    Form,
}

impl BodyFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            BodyFormat::Json => "json",
            BodyFormat::Form => "form",
        }
    }
}

impl fmt::Display for BodyFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Returned when a string is not a stored body format.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown body format {0:?}")]
pub struct UnknownBodyFormat(pub String);

impl FromStr for BodyFormat {
    type Err = UnknownBodyFormat;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "json" => Ok(BodyFormat::Json),
            "form" => Ok(BodyFormat::Form),
            other => Err(UnknownBodyFormat(other.to_owned())),
        }
    }
}

impl ToSql for BodyFormat {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_str()))
    }
}

impl FromSql for BodyFormat {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value
            .as_str()?
            .parse()
            .map_err(|e| FromSqlError::Other(Box::new(e)))
    }
}

/// One row of the sidebar list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointSummary {
    pub id: EndpointId,
    pub name: String,
    pub method: String,
    pub url: String,
    pub sort_order: i64,
}

/// A stored endpoint with its fields and headers in position order.
///
/// These are plain storage records; mapping to the `fakeman-core` domain
/// types happens in the app.
#[derive(Debug, Clone, PartialEq)]
pub struct EndpointRecord {
    pub id: EndpointId,
    pub name: String,
    pub method: String,
    pub url: String,
    pub body_format: BodyFormat,
    pub run_count: u32,
    pub delay_ms: u64,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
    pub fields: Vec<FieldRecord>,
    pub headers: Vec<HeaderRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldRecord {
    pub name: String,
    pub generator_key: String,
    pub params: serde_json::Value,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderRecord {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}
