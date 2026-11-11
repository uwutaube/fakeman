# fakeman
Postman but with fake data. Test your API endpoint with fake data.

## Development

Requires stable Rust. On Windows, use the MSVC toolchain with the Visual Studio "Desktop development with C++" workload; on macOS, the Xcode Command Line Tools.

```sh
cargo run -p fakeman-app
cargo test --workspace
```

See [docs/PLAN.md](docs/PLAN.md) for the architecture and phases.
