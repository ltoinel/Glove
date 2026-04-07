# Development Setup

## Prerequisites

- [Rust](https://rustup.rs/) 1.85+ (with `cargo-watch` for dev mode)
- [Node.js](https://nodejs.org/) 18+ with npm
- [Docker](https://www.docker.com/) (optional, for Valhalla)

## Quick Start

```bash
# Clone the repository
git clone https://github.com/ltoinel/Glove.git
cd Glove

# Download GTFS data
bin/download.sh gtfs

# Start in dev mode (auto-reload on file changes)
bin/start.sh --dev
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
npx eslint src/              # Lint (must pass in CI)
```

## CI Pipeline

GitHub Actions runs on every push to `master` and on pull requests:

1. **Backend**: format check, clippy, build, test
2. **Coverage**: cargo-tarpaulin with Codecov upload
3. **Frontend**: ESLint + Vite build

All checks must pass before merging.

## Useful Commands

```bash
# Run with debug logging
RUST_LOG=debug cargo run

# Run benchmarks
python3 bin/benchmark.py --rounds 10

# Start Valhalla for walk/bike/car routing
bin/valhalla.sh
```
