# Development Setup

## Prerequisites

- [Rust](https://rustup.rs/) 1.85+ (with `cargo-watch` for dev mode)
- [Node.js](https://nodejs.org/) 22+ with npm (required by `swagger-client`, a transitive dependency of the API docs viewer)
- [Docker](https://www.docker.com/) (optional, for Valhalla)

## Quick Start

```bash
# Clone the repository
git clone https://github.com/ltoinel/Glove.git
cd Glove

# Download GTFS data
bin/download.sh gtfs

# Start in dev mode (auto-reload on file changes)
bin/start.sh --dev    # https://portal.glove (HMR) + https://api.glove, through Caddy
```

`bin/start.sh` needs a one-time Caddy setup (ports, `/etc/hosts`, local CA) — see [Installation](../getting-started/installation.md#one-time-setup). `npm run dev` alone still works without Caddy, on http://localhost:3000.

## Pre-commit Lint

A versioned Git hook (`.githooks/pre-commit`) runs the CI lint steps before each commit — `cargo fmt --check` and `cargo clippy -- -D warnings` when Rust files are staged, `npm run lint` when `portal/` files are. Enable it once per clone:

```bash
git config core.hooksPath .githooks
```

`git commit --no-verify` skips it. Checks run on the working tree, so with partially staged files the result can differ from CI.

```admonish tip title="Match the CI toolchain"
CI lints with the latest stable Rust, whose Clippy may add lints your local toolchain does not know yet. Keep it current with `rustup update stable`.
```

## Backend Development

```bash
cargo build                  # Debug build
cargo build --release        # Release build
cargo test                   # Run all tests
cargo clippy -- -D warnings  # Lint (must pass in CI)
cargo fmt --check            # Format check (must pass in CI)
cargo fmt                    # Auto-format
```

The dev mode uses `cargo-watch` to recompile automatically on file changes:

```bash
cargo install cargo-watch
cargo watch -x run
```

## Frontend Development

```bash
cd portal
npm install                  # Install dependencies
npm run dev                  # Vite dev server on port 3000 with HMR
npm run build                # Production build to dist/
npm run lint                 # ESLint over the whole portal, config files included (CI)
npm test                     # vitest (CI)
```

## CI Pipeline

GitHub Actions runs on every push to `master` and on pull requests (`.github/workflows/ci.yml`). A first job detects which parts of the tree changed, and only the relevant jobs run:

| Job | Runs when | Checks |
|-----|-----------|--------|
| **Backend** | Rust changes | `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` |
| **MSRV** | Rust changes | `cargo check --locked` on Rust 1.88, the `rust-version` in `Cargo.toml` |
| **Supply chain** | Rust changes | `cargo deny check`: RustSec advisories, licenses, sources (`deny.toml`) |
| **Coverage** | Rust changes | cargo-tarpaulin, uploaded to Codecov |
| **Frontend** | `portal/` changes | `npm run lint`, `npm test`, `npm run build`, `npm audit` (production deps) |
| **Docker** | Rust, portal or `docker/` changes | Builds both images (no push), with layer cache shared with releases |

The documentation book is built on pull requests and deployed from `master` (`docs.yml`); both Docker images are published on each GitHub release (`docker.yml`).

- **Speed.** A new push to a pull request cancels the run in progress. Rust dependencies are cached with `rust-cache`, and tools are installed as prebuilt binaries.
- **Pinned actions.** Actions are pinned by commit SHA, and Dependabot keeps them, the crates, the npm packages and the base images up to date. It waits 7 days before proposing a new release.

All checks must pass before merging.

## Useful Commands

```bash
# Run with debug logging
RUST_LOG=debug cargo run

# Run benchmarks
python3 scripts/benchmark.py --rounds 10 --concurrency 1 --datetime <YYYYMMDDTHHMMSS in the GTFS window>

# Start Valhalla for walk/bike/car routing
bin/valhalla.sh

# Check which GTFS transfer pairs have indoor routing data in Valhalla
python3 bin/check_indoor.py
```

```admonish info title="Indoor Coverage Analysis"
The `check_indoor.py` script queries Valhalla for each GTFS transfer pair to determine which ones have indoor routing data available from OSM. This is useful for understanding indoor coverage in your deployment area.
```
