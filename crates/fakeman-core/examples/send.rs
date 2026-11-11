//! Sends an endpoint loaded from a JSON file and prints every run event.
//!
//! ```text
//! cargo run -p fakeman-core --example send -- crates/fakeman-core/examples/endpoint.json
//! ```
//!
//! The file holds one serialised [`Endpoint`]. It is read from disk rather than the database
//! so that the core stays independent of the store.

use std::error::Error;

use fakeman_core::{CancellationToken, Endpoint, RunEvent, RunRequest, Runner};

fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: cargo run -p fakeman-core --example send -- <endpoint.json>")?;
    let endpoint: Endpoint = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let runner = Runner::new()?;
    let mut events = runner.run(RunRequest::from(&endpoint), CancellationToken::new())?;
    while let Some(event) = events.blocking_recv() {
        match event {
            RunEvent::Started { total } => {
                println!("{} {} x{total}", endpoint.method.as_str(), endpoint.url);
            }
            RunEvent::RequestFinished {
                index,
                payload,
                outcome,
            } => {
                let latency = outcome.latency.as_millis();
                match &outcome.result {
                    Ok(response) => println!(
                        "#{index} {} · {latency} ms · {} B{}",
                        response.status,
                        response.size,
                        if response.truncated {
                            " (truncated)"
                        } else {
                            ""
                        }
                    ),
                    Err(error) => {
                        println!("#{index} {:?} error · {latency} ms: {error}", error.kind)
                    }
                }
                if let Some(payload) = payload {
                    println!("  sent: {}", payload.encoded);
                }
                if let Ok(response) = &outcome.result {
                    for (name, value) in &response.headers {
                        println!("  < {name}: {value}");
                    }
                    println!("{}", response.text());
                }
            }
            RunEvent::Finished { summary } => println!(
                "done: {} succeeded, {} failed, average {} ms, total {} ms",
                summary.succeeded,
                summary.failed,
                summary.average_latency.unwrap_or_default().as_millis(),
                summary.elapsed.as_millis()
            ),
            RunEvent::Cancelled => println!("cancelled"),
        }
    }
    Ok(())
}
