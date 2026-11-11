# fakeman
Postman but with fake data. Test your API endpoint with fake data.

## Development

Requires stable Rust. On Windows, use the MSVC toolchain with the Visual Studio "Desktop development with C++" workload; on macOS, the Xcode Command Line Tools.

```sh
cargo run -p fakeman-app
cargo test --workspace
```

See [docs/PLAN.md](docs/PLAN.md) for the architecture and phases.

## Enabling macOS

Windows is the only platform built and tested by default. The code stays cross-platform, and the CI workflow has a macOS job that is skipped unless you opt in in one of these ways:

- **Every run:** set the repository variable `ENABLE_MACOS` to `true` (Settings → Secrets and variables → Actions → Variables).
- **One run:** start the CI workflow manually (Actions → CI → Run workflow) and tick the `macos` input.
- **One pull request:** add the `macos` label to the PR.
