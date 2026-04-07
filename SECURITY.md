# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability in Glove, please report it responsibly. **Do not open a public GitHub issue.**

Instead, please send an email to **[ludovic@toinel.com]** with:

- A description of the vulnerability
- Steps to reproduce the issue
- The potential impact
- Any suggested fix (optional)

You should receive an acknowledgment within **48 hours**. We will work with you to understand the issue and coordinate a fix before any public disclosure.

## Disclosure Policy

- We follow a **coordinated disclosure** process.
- We aim to release a fix within **30 days** of confirming a vulnerability.
- Credit will be given to the reporter unless they prefer to remain anonymous.

## Security Considerations

### Architecture

Glove is a public transit journey planner that runs entirely in-memory. There is no database, no user authentication, and no persistent user data storage. The attack surface is primarily the HTTP API layer.

### API Surface

- All API endpoints are read-only (`GET`), except `/api/reload` (`POST`) which triggers a GTFS data reload.
- The `/api/reload` endpoint should be protected in production (e.g., behind a reverse proxy with authentication or restricted to trusted networks).
- Rate limiting is enabled to mitigate abuse.

### Data

- Glove processes publicly available GTFS (General Transit Feed Specification) data.
- No personal or sensitive user data is collected or stored.
- No cookies or sessions are used.

### Dependencies

- Rust dependencies are managed via Cargo and can be audited with [`cargo audit`](https://github.com/RustSec/cargo-audit).
- Frontend dependencies are managed via npm and can be audited with `npm audit`.
- We recommend running these audits regularly and before each release.

### Deployment

- In production, Glove should be deployed behind a reverse proxy (e.g., Nginx, Caddy) that handles TLS termination.
- CORS is configured to restrict cross-origin requests. Review the `cors` settings in `config.yaml` for your deployment.
- Docker images are available for containerized deployment with reduced attack surface.

## Best Practices for Operators

1. **Restrict `/api/reload`** to authorized clients only.
2. **Enable TLS** via a reverse proxy — Glove does not handle TLS natively.
3. **Keep dependencies updated** — run `cargo audit` and `npm audit` periodically.
4. **Use the Docker image** in production for isolation.
5. **Review `config.yaml`** settings, especially CORS origins and rate limits, before deploying.

## Security Updates

Security patches will be released as new versions. Watch the repository releases or subscribe to notifications to stay informed.
