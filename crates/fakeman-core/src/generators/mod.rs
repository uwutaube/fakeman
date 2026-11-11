//! The generator registry: every generator a field can be bound to, keyed by a stable
//! Faker-style string such as `person.firstName`.
//!
//! Keys are stored in the database and are a public contract: once shipped, a key is never
//! renamed, only deprecated.

mod catalog;

use chrono::{DateTime, Utc};
use serde_json::{Map, Number, Value};

use crate::model::GeneratorRef;
use crate::rng::GenContext;

pub use catalog::CATALOG;

/// One entry of the catalog.
pub struct GeneratorDef {
    pub key: &'static str,
    pub category: &'static str,
    pub label: &'static str,
    /// The JSON type every generated value has.
    pub output: ValueKind,
    pub params: &'static [ParamDef],
    /// Cross-parameter rule (for example `min <= max`), run after each parameter is checked.
    pub check: Option<ParamCheck>,
    pub generate: fn(&Params, &mut GenContext) -> Value,
}

impl std::fmt::Debug for GeneratorDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeneratorDef")
            .field("key", &self.key)
            .field("category", &self.category)
            .field("label", &self.label)
            .field("output", &self.output)
            .field("params", &self.params)
            .finish_non_exhaustive()
    }
}

pub type ParamCheck = fn(&Params) -> Result<(), ParamIssue>;

/// The JSON type of a generated value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    String,
    /// A JSON number that is a whole number.
    Integer,
    /// Any JSON number.
    Number,
    Boolean,
    Null,
}

impl ValueKind {
    pub fn matches(self, value: &Value) -> bool {
        match self {
            ValueKind::String => value.is_string(),
            ValueKind::Integer => value.is_i64() || value.is_u64(),
            ValueKind::Number => value.is_number(),
            ValueKind::Boolean => value.is_boolean(),
            ValueKind::Null => value.is_null(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamDef {
    pub name: &'static str,
    pub label: &'static str,
    pub kind: ParamKind,
}

/// The type of a parameter, its default, and for integers the allowed range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamKind {
    Int {
        default: i64,
        min: i64,
        max: i64,
    },
    Number {
        default: f64,
    },
    Bool {
        default: bool,
    },
    String {
        default: &'static str,
    },
    /// An RFC 3339 / ISO-8601 timestamp such as `2024-01-31T12:00:00Z`.
    DateTime {
        default: &'static str,
    },
}

impl ParamKind {
    fn default_value(self) -> Value {
        match self {
            ParamKind::Int { default, .. } => Value::from(default),
            ParamKind::Number { default } => number_value(default),
            ParamKind::Bool { default } => Value::Bool(default),
            ParamKind::String { default } | ParamKind::DateTime { default } => Value::from(default),
        }
    }

    fn check(self, value: &Value) -> Result<(), String> {
        match self {
            ParamKind::Int { min, max, .. } => match value.as_i64() {
                Some(v) if (min..=max).contains(&v) => Ok(()),
                Some(_) => Err(format!("must be between {min} and {max}")),
                None => Err("must be a whole number".into()),
            },
            ParamKind::Number { .. } if value.is_number() => Ok(()),
            ParamKind::Number { .. } => Err("must be a number".into()),
            ParamKind::Bool { .. } if value.is_boolean() => Ok(()),
            ParamKind::Bool { .. } => Err("must be true or false".into()),
            ParamKind::String { .. } if value.is_string() => Ok(()),
            ParamKind::String { .. } => Err("must be text".into()),
            ParamKind::DateTime { .. } => match value.as_str().map(parse_datetime) {
                Some(Some(_)) => Ok(()),
                _ => Err("must be an ISO-8601 date-time such as 2024-01-31T12:00:00Z".into()),
            },
        }
    }
}

/// A rule violation for one parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamIssue {
    pub param: &'static str,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GeneratorError {
    #[error("unknown generator `{0}`")]
    UnknownKey(String),
    #[error("parameters for `{key}` must be a JSON object")]
    ParamsNotObject { key: String },
    #[error("parameter `{param}` of `{key}` {reason}")]
    InvalidParam {
        key: String,
        param: &'static str,
        reason: String,
    },
}

/// Validated parameters with every default filled in.
#[derive(Debug, Clone, PartialEq)]
pub struct Params(Map<String, Value>);

impl Params {
    pub fn get(&self, name: &str) -> &Value {
        self.0.get(name).unwrap_or(&Value::Null)
    }

    pub fn int(&self, name: &str) -> i64 {
        self.get(name).as_i64().unwrap_or_default()
    }

    pub fn float(&self, name: &str) -> f64 {
        self.get(name).as_f64().unwrap_or_default()
    }

    pub fn bool(&self, name: &str) -> bool {
        self.get(name).as_bool().unwrap_or_default()
    }

    pub fn str(&self, name: &str) -> &str {
        self.get(name).as_str().unwrap_or_default()
    }

    pub fn datetime(&self, name: &str) -> DateTime<Utc> {
        self.get(name)
            .as_str()
            .and_then(parse_datetime)
            .unwrap_or_default()
    }
}

impl GeneratorDef {
    /// Checks `params` against this generator's parameter definitions and fills in defaults.
    /// `null` means "no parameters"; parameters this generator does not know are ignored, so
    /// endpoints saved by a newer version still load.
    pub fn resolve_params(&self, params: &Value) -> Result<Params, GeneratorError> {
        let given = match params {
            Value::Null => &Map::new(),
            Value::Object(map) => map,
            _ => {
                return Err(GeneratorError::ParamsNotObject {
                    key: self.key.into(),
                });
            }
        };
        let mut resolved = Map::new();
        for def in self.params {
            let value = match given.get(def.name) {
                None | Some(Value::Null) => def.kind.default_value(),
                Some(value) => {
                    def.kind
                        .check(value)
                        .map_err(|reason| self.invalid(def.name, reason))?;
                    value.clone()
                }
            };
            resolved.insert(def.name.into(), value);
        }
        let params = Params(resolved);
        if let Some(check) = self.check {
            check(&params).map_err(|issue| self.invalid(issue.param, issue.reason))?;
        }
        Ok(params)
    }

    fn invalid(&self, param: &'static str, reason: String) -> GeneratorError {
        GeneratorError::InvalidParam {
            key: self.key.into(),
            param,
            reason,
        }
    }
}

/// Every generator, in picker order.
pub fn catalog() -> &'static [GeneratorDef] {
    CATALOG
}

pub fn lookup(key: &str) -> Result<&'static GeneratorDef, GeneratorError> {
    CATALOG
        .iter()
        .find(|def| def.key == key)
        .ok_or_else(|| GeneratorError::UnknownKey(key.into()))
}

/// One group of the generator picker.
#[derive(Debug)]
pub struct Category {
    pub name: &'static str,
    pub generators: Vec<&'static GeneratorDef>,
}

/// The catalog grouped by category, in catalog order.
pub fn categories() -> Vec<Category> {
    let mut groups: Vec<Category> = Vec::new();
    for def in CATALOG {
        match groups.iter_mut().find(|group| group.name == def.category) {
            Some(group) => group.generators.push(def),
            None => groups.push(Category {
                name: def.category,
                generators: vec![def],
            }),
        }
    }
    groups
}

/// Checks that the key exists and its parameters are valid.
pub fn validate(generator: &GeneratorRef) -> Result<(), GeneratorError> {
    lookup(&generator.key)?.resolve_params(&generator.params)?;
    Ok(())
}

/// Generates one value.
pub fn generate(generator: &GeneratorRef, ctx: &mut GenContext) -> Result<Value, GeneratorError> {
    let def = lookup(&generator.key)?;
    let params = def.resolve_params(&generator.params)?;
    Ok((def.generate)(&params, ctx))
}

fn parse_datetime(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// A whole-valued float becomes a JSON integer, so a default of `0` serialises as `0`.
fn number_value(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        Value::from(value as i64)
    } else {
        Number::from_f64(value).map_or(Value::Null, Value::Number)
    }
}

#[cfg(test)]
mod tests;
