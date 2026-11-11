//! Sends a [`RunRequest`] one or more times, one request at a time, and streams the results
//! as [`RunEvent`]s.
//!
//! HTTP runs on a tokio runtime owned by the [`Runner`]. Events go out over an unbounded
//! channel whose receiver does not need tokio, so the UI can await it on its own executor
//! (or poll it with `try_recv`).

use std::borrow::Cow;
use std::error::Error as _;
use std::time::{Duration, Instant};

use reqwest::Url;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
pub use tokio_util::sync::CancellationToken;

use crate::body::{BodyError, build_body, check_names, content_type, encode_body};
use crate::generators::validate;
use crate::model::{BodyFormat, Endpoint, Field, Header, RunSettings};
use crate::rng::GenContext;

/// The receiving end of a run: every event of one run, then the channel closes.
pub type RunEvents = UnboundedReceiver<RunEvent>;

/// A snapshot of an endpoint taken when Send is pressed, so edits during a run do not affect it.
#[derive(Debug, Clone, PartialEq)]
pub struct RunRequest {
    pub url: String,
    pub body_format: BodyFormat,
    pub headers: Vec<Header>,
    pub fields: Vec<Field>,
    pub run: RunSettings,
}

impl From<&Endpoint> for RunRequest {
    fn from(endpoint: &Endpoint) -> Self {
        Self {
            url: endpoint.url.clone(),
            body_format: endpoint.body_format,
            headers: endpoint.headers.clone(),
            fields: endpoint.fields.clone(),
            run: endpoint.run,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunnerConfig {
    /// Time allowed to establish a connection, TLS included.
    pub connect_timeout: Duration,
    /// Time allowed for a whole request, from connecting to the last body byte.
    pub request_timeout: Duration,
    /// Response bytes kept per request; the rest is counted in [`Response::size`] but dropped.
    pub max_body_bytes: usize,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            max_body_bytes: 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RunEvent {
    Started {
        total: u32,
    },
    /// One request is done. `index` counts from 1. `payload` is `None` only when the body
    /// could not be generated, in which case `outcome` holds that error.
    RequestFinished {
        index: u32,
        payload: Option<Payload>,
        outcome: Outcome,
    },
    Finished {
        summary: RunSummary,
    },
    /// The run was cancelled; no further requests were sent and no `Finished` follows.
    Cancelled,
}

/// The body sent by one request: the generated value and its exact encoded text.
#[derive(Debug, Clone, PartialEq)]
pub struct Payload {
    pub body: Value,
    pub encoded: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// From starting the request to the last response byte, or to the error.
    pub latency: Duration,
    pub result: Result<Response, RequestError>,
}

impl Outcome {
    /// True for a received 2xx response.
    pub fn is_success(&self) -> bool {
        matches!(&self.result, Ok(response) if response.is_success())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    /// At most [`RunnerConfig::max_body_bytes`] bytes of the body.
    pub body: Vec<u8>,
    /// Full size of the body in bytes, including any part that was not kept.
    pub size: u64,
    pub truncated: bool,
}

impl Response {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The kept body as text, with invalid UTF-8 replaced.
    pub fn text(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.body)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct RequestError {
    pub kind: ErrorKind,
    /// The error and all of its causes, for display.
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// The body could not be generated or encoded.
    Body,
    /// The connection could not be established (refused, DNS, TLS handshake, ...).
    Connect,
    /// The connect or request timeout ran out.
    Timeout,
    /// Anything else, such as the connection dropping mid-response.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunSummary {
    pub total: u32,
    /// Requests that received a 2xx response.
    pub succeeded: u32,
    /// Requests that failed or received a non-2xx response.
    pub failed: u32,
    /// Mean latency over all requests, `None` when nothing was sent.
    pub average_latency: Option<Duration>,
    /// Wall time of the whole run, delays included.
    pub elapsed: Duration,
}

/// Failures to set up the runner itself.
#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("could not start the HTTP runtime: {0}")]
    Runtime(#[source] std::io::Error),
    #[error("could not create the HTTP client: {0}")]
    Client(#[source] reqwest::Error),
}

/// Problems found before a run starts; nothing has been sent when these are returned.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RunError {
    #[error("invalid URL `{url}`: {reason}")]
    InvalidUrl { url: String, reason: String },
    #[error("only http and https URLs can be sent, not `{0}`")]
    UnsupportedScheme(String),
    #[error(transparent)]
    Body(#[from] BodyError),
    #[error("header `{0}` is not a valid header name")]
    InvalidHeaderName(String),
    #[error("header `{0}` has a value that cannot be sent")]
    InvalidHeaderValue(String),
}

/// Owns the tokio runtime and HTTP client that every run uses.
pub struct Runner {
    runtime: Runtime,
    client: reqwest::Client,
    max_body_bytes: usize,
}

impl Runner {
    pub fn new() -> Result<Self, RunnerError> {
        Self::with_config(RunnerConfig::default())
    }

    pub fn with_config(config: RunnerConfig) -> Result<Self, RunnerError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("fakeman-runner")
            .enable_all()
            .build()
            .map_err(RunnerError::Runtime)?;
        let client = {
            let _guard = runtime.enter();
            reqwest::Client::builder()
                .connect_timeout(config.connect_timeout)
                .timeout(config.request_timeout)
                .user_agent(concat!("fakeman/", env!("CARGO_PKG_VERSION")))
                .build()
                .map_err(RunnerError::Client)?
        };
        Ok(Self {
            runtime,
            client,
            max_body_bytes: config.max_body_bytes,
        })
    }

    /// Checks the request, then sends it `run.count` times in the background.
    ///
    /// Each send generates a fresh body from one [`GenContext`] per run. Enabled headers are
    /// applied, with `Content-Type` chosen by [`content_type`]. Requests wait
    /// `run.delay_ms` between each other, never after the last. Failed and non-2xx requests
    /// are recorded and the run goes on. Cancelling `cancel` stops the run during a delay
    /// and aborts a request in flight.
    pub fn run(
        &self,
        request: RunRequest,
        cancel: CancellationToken,
    ) -> Result<RunEvents, RunError> {
        let url = parse_url(&request.url)?;
        check_names(&request.fields, request.body_format)?;
        for field in request.fields.iter().filter(|field| field.enabled) {
            validate(&field.generator).map_err(|source| BodyError::Generator {
                field: field.name.clone(),
                source,
            })?;
        }
        let headers = header_map(&request.headers, request.body_format)?;
        let (events, receiver) = unbounded_channel();
        let job = Job {
            client: self.client.clone(),
            url,
            headers,
            max_body_bytes: self.max_body_bytes,
            request,
        };
        self.runtime.spawn(job.execute(events, cancel));
        Ok(receiver)
    }
}

fn parse_url(text: &str) -> Result<Url, RunError> {
    let url = Url::parse(text.trim()).map_err(|error| RunError::InvalidUrl {
        url: text.into(),
        reason: error.to_string(),
    })?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        scheme => Err(RunError::UnsupportedScheme(scheme.into())),
    }
}

fn header_map(headers: &[Header], format: BodyFormat) -> Result<HeaderMap, RunError> {
    let mut map = HeaderMap::new();
    for header in headers.iter().filter(|header| header.enabled) {
        let key = header.key.trim();
        if key.is_empty() || key.eq_ignore_ascii_case("content-type") {
            continue;
        }
        let name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|_| RunError::InvalidHeaderName(key.into()))?;
        let value = HeaderValue::from_str(header.value.trim())
            .map_err(|_| RunError::InvalidHeaderValue(key.into()))?;
        map.append(name, value);
    }
    let value = HeaderValue::from_str(content_type(headers, format))
        .map_err(|_| RunError::InvalidHeaderValue("Content-Type".into()))?;
    map.insert(CONTENT_TYPE, value);
    Ok(map)
}

struct Job {
    client: reqwest::Client,
    url: Url,
    headers: HeaderMap,
    max_body_bytes: usize,
    request: RunRequest,
}

impl Job {
    async fn execute(self, events: UnboundedSender<RunEvent>, cancel: CancellationToken) {
        let total = self.request.run.count;
        let delay = Duration::from_millis(self.request.run.delay_ms);
        let started = Instant::now();
        let mut ctx = GenContext::new();
        let (mut succeeded, mut latency) = (0, Duration::ZERO);
        let _ = events.send(RunEvent::Started { total });
        for index in 1..=total {
            if index > 1 && !delay.is_zero() {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => {}
                    _ = tokio::time::sleep(delay) => {}
                }
            }
            if cancel.is_cancelled() {
                let _ = events.send(RunEvent::Cancelled);
                return;
            }
            let payload = self.payload(&mut ctx);
            let sent = Instant::now();
            let result = match &payload {
                Err(error) => Err(error.clone()),
                Ok(payload) => tokio::select! {
                    biased;
                    _ = cancel.cancelled() => {
                        let _ = events.send(RunEvent::Cancelled);
                        return;
                    }
                    result = self.send(payload) => result,
                },
            };
            let outcome = Outcome {
                latency: sent.elapsed(),
                result,
            };
            latency += outcome.latency;
            succeeded += u32::from(outcome.is_success());
            let _ = events.send(RunEvent::RequestFinished {
                index,
                payload: payload.ok(),
                outcome,
            });
        }
        let summary = RunSummary {
            total,
            succeeded,
            failed: total - succeeded,
            average_latency: latency.checked_div(total),
            elapsed: started.elapsed(),
        };
        let _ = events.send(RunEvent::Finished { summary });
    }

    fn payload(&self, ctx: &mut GenContext) -> Result<Payload, RequestError> {
        let format = self.request.body_format;
        let body_error = |error: &dyn std::error::Error| RequestError {
            kind: ErrorKind::Body,
            message: error.to_string(),
        };
        let body = build_body(&self.request.fields, format, ctx).map_err(|e| body_error(&e))?;
        let encoded = encode_body(&body, format).map_err(|e| body_error(&e))?;
        Ok(Payload { body, encoded })
    }

    async fn send(&self, payload: &Payload) -> Result<Response, RequestError> {
        let mut response = self
            .client
            .post(self.url.clone())
            .headers(self.headers.clone())
            .body(payload.encoded.clone())
            .send()
            .await
            .map_err(request_error)?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
                (name.as_str().to_owned(), value)
            })
            .collect();
        let mut body = Vec::new();
        let mut size = 0u64;
        while let Some(chunk) = response.chunk().await.map_err(request_error)? {
            size += chunk.len() as u64;
            let room = self.max_body_bytes.saturating_sub(body.len());
            body.extend_from_slice(&chunk[..chunk.len().min(room)]);
        }
        Ok(Response {
            status,
            headers,
            truncated: size > body.len() as u64,
            body,
            size,
        })
    }
}

fn request_error(error: reqwest::Error) -> RequestError {
    let kind = if error.is_timeout() {
        ErrorKind::Timeout
    } else if error.is_connect() {
        ErrorKind::Connect
    } else {
        ErrorKind::Other
    };
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    RequestError { kind, message }
}
