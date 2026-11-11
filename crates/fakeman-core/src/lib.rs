//! Domain model, generator catalog, body builder and HTTP runner for fakeman.

pub mod body;
pub mod generators;
pub mod model;
pub mod rng;
pub mod runner;

pub use body::{BodyError, EncodeError, build_body, check_names, content_type, encode_body};
pub use generators::{
    Category, GeneratorDef, GeneratorError, ParamDef, ParamKind, Params, ValueKind, catalog,
    categories, generate, lookup, validate,
};
pub use model::{
    BodyFormat, Endpoint, EndpointId, Field, GeneratorRef, Header, Method, RunSettings,
};
pub use rng::GenContext;
pub use runner::{
    CancellationToken, ErrorKind, Outcome, Payload, RequestError, Response, RunError, RunEvent,
    RunEvents, RunRequest, RunSummary, Runner, RunnerConfig, RunnerError,
};
