# fakeman
Postman but with fake data. Test your API endpoint with fake data.

## Development

Requires stable Rust. On Windows, use the MSVC toolchain with the Visual Studio "Desktop development with C++" workload; on macOS, the Xcode Command Line Tools.

```sh
cargo run -p fakeman-app
cargo test --workspace
```

See [docs/PLAN.md](docs/PLAN.md) for the architecture and phases.

## CI

CI is manual while fakeman is in development: nothing runs on push or on pull requests. Run it from the Actions tab (Actions → CI → Run workflow), or with:

```sh
gh workflow run ci.yml --ref <branch>
gh workflow run ci.yml --ref <branch> -f macos=true   # also run macOS
```

Windows is the only platform checked by default. macOS uses the same steps and runs when you tick the `macos` input on a manual run, or on every run once the repository variable `ENABLE_MACOS` is `true` (Settings → Secrets and variables → Actions → Variables).

To make CI automatic again, add the `push` (branches: `main`) and `pull_request` triggers back to `.github/workflows/ci.yml`.
