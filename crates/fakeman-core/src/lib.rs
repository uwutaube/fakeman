//! Domain model, generator catalog and body builder for fakeman.

pub mod body;
pub mod generators;
pub mod model;
pub mod rng;

pub use body::{BodyError, EncodeError, build_body, check_names, content_type, encode_body};
pub use generators::{
    Category, GeneratorDef, GeneratorError, ParamDef, ParamKind, Params, ValueKind, catalog,
    categories, generate, lookup, validate,
};
pub use model::{
    BodyFormat, Endpoint, EndpointId, Field, GeneratorRef, Header, Method, RunSettings,
};
pub use rng::GenContext;
