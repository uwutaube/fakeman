//! Runner tests against a local wiremock server.

use std::collections::HashSet;
use std::net::TcpListener;
use std::time::{Duration, Instant};

use fakeman_core::{
    BodyError, BodyFormat, CancellationToken, ErrorKind, Field, GeneratorError, GeneratorRef,
    Header, RunError, RunEvent, RunEvents, RunRequest, RunSettings, RunSummary, Runner,
    RunnerConfig,
};
use serde_json::{Value, json};
use tokio::runtime::Runtime;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

fn field(name: &str, key: &str) -> Field {
    Field {
        name: name.into(),
        generator: GeneratorRef::new(key),
        enabled: true,
    }
}

fn header(key: &str, value: &str, enabled: bool) -> Header {
    Header {
        key: key.into(),
        value: value.into(),
        enabled,
    }
}

fn request(url: String, format: BodyFormat, count: u32, delay_ms: u64) -> RunRequest {
    RunRequest {
        url,
        body_format: format,
        headers: vec![
            header("Authorization", "Bearer secret", true),
            header("X-Disabled", "nope", false),
        ],
        fields: vec![
            field("firstname", "person.firstName"),
            field("id", "datatype.uuid"),
            field("address.city", "location.city"),
        ],
        run: RunSettings { count, delay_ms },
    }
}

/// wiremock needs a tokio runtime of its own; the runner keeps its runtime private.
fn server(rt: &Runtime, response: ResponseTemplate) -> MockServer {
    rt.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/users"))
            .respond_with(response)
            .mount(&server)
            .await;
        server
    })
}

fn received(rt: &Runtime, server: &MockServer) -> Vec<Request> {
    rt.block_on(server.received_requests()).unwrap_or_default()
}

/// Collects every event with the time it arrived, until the channel closes.
fn collect(mut events: RunEvents) -> Vec<(Instant, RunEvent)> {
    let mut all = Vec::new();
    while let Some(event) = events.blocking_recv() {
        all.push((Instant::now(), event));
    }
    all
}

fn summary(events: &[(Instant, RunEvent)]) -> RunSummary {
    match events.last() {
        Some((_, RunEvent::Finished { summary })) => *summary,
        other => panic!("run did not finish: {other:?}"),
    }
}

fn sends_distinct_bodies(format: BodyFormat, expected_type: &str) {
    let rt = Runtime::new().unwrap();
    let server = server(&rt, ResponseTemplate::new(201).set_body_string("created"));
    let runner = Runner::new().unwrap();
    let url = format!("{}/users", server.uri());
    let events = runner
        .run(request(url, format, 5, 0), CancellationToken::new())
        .unwrap();
    let events = collect(events);

    assert_eq!(events.len(), 7);
    assert_eq!(events[0].1, RunEvent::Started { total: 5 });
    let mut payloads = Vec::new();
    for (expected_index, (_, event)) in (1..).zip(&events[1..6]) {
        let RunEvent::RequestFinished {
            index,
            payload,
            outcome,
        } = event
        else {
            panic!("expected RequestFinished, got {event:?}");
        };
        assert_eq!(*index, expected_index);
        let response = outcome.result.as_ref().unwrap();
        assert_eq!(response.status, 201);
        assert_eq!(response.text(), "created");
        assert_eq!(response.size, 7);
        assert!(!response.truncated);
        assert!(outcome.is_success());
        payloads.push(payload.clone().unwrap());
    }
    let summary = summary(&events);
    assert_eq!(
        (summary.total, summary.succeeded, summary.failed),
        (5, 5, 0)
    );
    assert!(summary.average_latency.is_some());

    let requests = received(&rt, &server);
    assert_eq!(requests.len(), 5);
    let bodies: HashSet<_> = requests.iter().map(|r| r.body.clone()).collect();
    assert_eq!(
        bodies.len(),
        5,
        "every request carries a freshly generated body"
    );
    for (sent, payload) in requests.iter().zip(&payloads) {
        assert_eq!(sent.headers["content-type"], expected_type);
        assert_eq!(sent.headers["authorization"], "Bearer secret");
        assert!(!sent.headers.contains_key("x-disabled"));
        assert_eq!(
            String::from_utf8(sent.body.clone()).unwrap(),
            payload.encoded
        );
        let decoded: Value = match format {
            BodyFormat::Json => serde_json::from_slice(&sent.body).unwrap(),
            BodyFormat::FormUrlEncoded => form_urlencoded::parse(&sent.body)
                .map(|(key, value)| (key.into_owned(), Value::String(value.into_owned())))
                .collect(),
        };
        assert_eq!(decoded, payload.body);
    }
}

#[test]
fn json_runs_send_n_distinct_bodies_as_application_json() {
    sends_distinct_bodies(BodyFormat::Json, "application/json");
}

#[test]
fn form_runs_send_n_distinct_bodies_as_form_urlencoded() {
    sends_distinct_bodies(
        BodyFormat::FormUrlEncoded,
        "application/x-www-form-urlencoded",
    );
}

#[test]
fn json_bodies_nest_dotted_names_and_form_bodies_keep_them_flat() {
    let rt = Runtime::new().unwrap();
    let server = server(&rt, ResponseTemplate::new(200));
    let runner = Runner::new().unwrap();
    for format in [BodyFormat::Json, BodyFormat::FormUrlEncoded] {
        let url = format!("{}/users", server.uri());
        let events = collect(
            runner
                .run(request(url, format, 1, 0), CancellationToken::new())
                .unwrap(),
        );
        let RunEvent::RequestFinished { payload, .. } = &events[1].1 else {
            panic!("expected RequestFinished");
        };
        let body = &payload.as_ref().unwrap().body;
        match format {
            BodyFormat::Json => assert!(body["address"]["city"].is_string()),
            BodyFormat::FormUrlEncoded => assert!(body["address.city"].is_string()),
        }
    }
}

#[test]
fn user_content_type_header_overrides_the_default() {
    let rt = Runtime::new().unwrap();
    let server = server(&rt, ResponseTemplate::new(200));
    let runner = Runner::new().unwrap();
    let mut run = request(format!("{}/users", server.uri()), BodyFormat::Json, 1, 0);
    run.headers
        .push(header("content-type", "application/vnd.api+json", true));
    collect(runner.run(run, CancellationToken::new()).unwrap());
    let requests = received(&rt, &server);
    assert_eq!(requests.len(), 1);
    let values: Vec<_> = requests[0].headers.get_all("content-type").iter().collect();
    assert_eq!(values, ["application/vnd.api+json"]);
}

#[test]
fn delay_is_waited_between_requests_but_not_after_the_last() {
    let rt = Runtime::new().unwrap();
    let server = server(&rt, ResponseTemplate::new(200));
    let runner = Runner::new().unwrap();
    let delay = Duration::from_millis(300);
    let url = format!("{}/users", server.uri());
    let events = collect(
        runner
            .run(
                request(url, BodyFormat::Json, 3, 300),
                CancellationToken::new(),
            )
            .unwrap(),
    );
    assert_eq!(events.len(), 5);
    for pair in events[1..4].windows(2) {
        let gap = pair[1].0 - pair[0].0;
        assert!(gap >= delay, "gap between requests was {gap:?}");
        assert!(gap < delay * 3, "gap between requests was {gap:?}");
    }
    let tail = events[4].0 - events[3].0;
    assert!(
        tail < delay / 2,
        "run waited {tail:?} after the last request"
    );
    let summary = summary(&events);
    assert!(summary.elapsed >= delay * 2);
    assert_eq!(received(&rt, &server).len(), 3);
}

#[test]
fn cancel_during_a_delay_stops_the_batch() {
    let rt = Runtime::new().unwrap();
    let server = server(&rt, ResponseTemplate::new(200));
    let runner = Runner::new().unwrap();
    let cancel = CancellationToken::new();
    let url = format!("{}/users", server.uri());
    let mut events = runner
        .run(request(url, BodyFormat::Json, 10, 5_000), cancel.clone())
        .unwrap();
    assert_eq!(
        events.blocking_recv(),
        Some(RunEvent::Started { total: 10 })
    );
    assert!(matches!(
        events.blocking_recv(),
        Some(RunEvent::RequestFinished { index: 1, .. })
    ));
    let cancelled_at = Instant::now();
    cancel.cancel();
    assert_eq!(events.blocking_recv(), Some(RunEvent::Cancelled));
    assert!(cancelled_at.elapsed() < Duration::from_secs(1));
    assert_eq!(events.blocking_recv(), None);
    assert_eq!(received(&rt, &server).len(), 1);
}

#[test]
fn cancel_aborts_the_request_in_flight() {
    let rt = Runtime::new().unwrap();
    let slow = ResponseTemplate::new(200).set_delay(Duration::from_secs(20));
    let server = server(&rt, slow);
    let runner = Runner::new().unwrap();
    let cancel = CancellationToken::new();
    let url = format!("{}/users", server.uri());
    let mut events = runner
        .run(
            request(url, BodyFormat::FormUrlEncoded, 3, 0),
            cancel.clone(),
        )
        .unwrap();
    assert_eq!(events.blocking_recv(), Some(RunEvent::Started { total: 3 }));
    std::thread::sleep(Duration::from_millis(300));
    let cancelled_at = Instant::now();
    cancel.cancel();
    assert_eq!(events.blocking_recv(), Some(RunEvent::Cancelled));
    assert!(cancelled_at.elapsed() < Duration::from_secs(1));
    assert_eq!(events.blocking_recv(), None);
    assert_eq!(received(&rt, &server).len(), 1);
}

#[test]
fn non_2xx_responses_are_recorded_and_the_batch_continues() {
    let rt = Runtime::new().unwrap();
    let server = rt.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .up_to_n_times(2)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        server
    });
    let runner = Runner::new().unwrap();
    let events = collect(
        runner
            .run(
                request(server.uri(), BodyFormat::Json, 3, 0),
                CancellationToken::new(),
            )
            .unwrap(),
    );
    let statuses: Vec<_> = events
        .iter()
        .filter_map(|(_, event)| match event {
            RunEvent::RequestFinished { outcome, .. } => {
                Some(outcome.result.as_ref().unwrap().status)
            }
            _ => None,
        })
        .collect();
    assert_eq!(statuses, [500, 500, 200]);
    let summary = summary(&events);
    assert_eq!((summary.succeeded, summary.failed), (1, 2));
}

#[test]
fn connection_errors_are_recorded_and_the_batch_continues() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let runner = Runner::new().unwrap();
    let url = format!("http://127.0.0.1:{port}/users");
    let events = collect(
        runner
            .run(
                request(url, BodyFormat::Json, 3, 0),
                CancellationToken::new(),
            )
            .unwrap(),
    );
    assert_eq!(events.len(), 5);
    for (_, event) in &events[1..4] {
        let RunEvent::RequestFinished {
            payload, outcome, ..
        } = event
        else {
            panic!("expected RequestFinished, got {event:?}");
        };
        assert!(payload.is_some());
        let error = outcome.result.as_ref().unwrap_err();
        assert_eq!(error.kind, ErrorKind::Connect, "{}", error.message);
        assert!(!outcome.is_success());
    }
    let summary = summary(&events);
    assert_eq!((summary.succeeded, summary.failed), (0, 3));
}

#[test]
fn timeouts_are_recorded_and_the_batch_continues() {
    let rt = Runtime::new().unwrap();
    let slow = ResponseTemplate::new(200).set_delay(Duration::from_secs(5));
    let server = server(&rt, slow);
    let runner = Runner::with_config(RunnerConfig {
        request_timeout: Duration::from_millis(200),
        ..RunnerConfig::default()
    })
    .unwrap();
    let url = format!("{}/users", server.uri());
    let events = collect(
        runner
            .run(
                request(url, BodyFormat::Json, 2, 0),
                CancellationToken::new(),
            )
            .unwrap(),
    );
    assert_eq!(events.len(), 4);
    for (_, event) in &events[1..3] {
        let RunEvent::RequestFinished { outcome, .. } = event else {
            panic!("expected RequestFinished, got {event:?}");
        };
        assert_eq!(
            outcome.result.as_ref().unwrap_err().kind,
            ErrorKind::Timeout
        );
        assert!(outcome.latency < Duration::from_secs(2));
    }
    assert_eq!(summary(&events).failed, 2);
}

#[test]
fn response_bodies_are_truncated_past_the_limit() {
    let rt = Runtime::new().unwrap();
    let big = ResponseTemplate::new(200)
        .insert_header("x-request-id", "abc")
        .set_body_string("x".repeat(5_000));
    let server = server(&rt, big);
    let runner = Runner::with_config(RunnerConfig {
        max_body_bytes: 1_024,
        ..RunnerConfig::default()
    })
    .unwrap();
    let url = format!("{}/users", server.uri());
    let events = collect(
        runner
            .run(
                request(url, BodyFormat::Json, 1, 0),
                CancellationToken::new(),
            )
            .unwrap(),
    );
    let RunEvent::RequestFinished { outcome, .. } = &events[1].1 else {
        panic!("expected RequestFinished");
    };
    let response = outcome.result.as_ref().unwrap();
    assert_eq!(response.body.len(), 1_024);
    assert_eq!(response.size, 5_000);
    assert!(response.truncated);
    assert!(
        response
            .headers
            .contains(&("x-request-id".into(), "abc".into()))
    );
}

#[test]
fn invalid_requests_are_rejected_before_sending() {
    let runner = Runner::new().unwrap();
    let cancel = CancellationToken::new;
    let run = |url: &str| request(url.into(), BodyFormat::Json, 1, 0);
    assert!(matches!(
        runner.run(run("not a url"), cancel()),
        Err(RunError::InvalidUrl { .. })
    ));
    assert_eq!(
        runner.run(run("ftp://example.com"), cancel()).unwrap_err(),
        RunError::UnsupportedScheme("ftp".into())
    );
    let mut bad_field = run("http://127.0.0.1:1/");
    bad_field.fields.push(Field {
        name: "age".into(),
        generator: GeneratorRef::with_params("number.int", json!({ "min": 5, "max": 1 })),
        enabled: true,
    });
    assert!(matches!(
        runner.run(bad_field, cancel()),
        Err(RunError::Body(BodyError::Generator { field, .. })) if field == "age"
    ));
    let mut unknown = run("http://127.0.0.1:1/");
    unknown.fields.push(field("x", "no.suchKey"));
    assert_eq!(
        runner.run(unknown, cancel()).unwrap_err(),
        RunError::Body(BodyError::Generator {
            field: "x".into(),
            source: GeneratorError::UnknownKey("no.suchKey".into()),
        })
    );
    let mut bad_header = run("http://127.0.0.1:1/");
    bad_header.headers.push(header("Bad Header", "x", true));
    assert_eq!(
        runner.run(bad_header, cancel()).unwrap_err(),
        RunError::InvalidHeaderName("Bad Header".into())
    );
}
