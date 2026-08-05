# OxideAuth API

REST API for the OxideAuth IAM platform — OIDC authentication, role-based authorization, workspace & account management.

## Tech Stack

- **Framework:** [Axum](https://github.com/tokio-rs/axum) 0.7
- **Runtime:** [Tokio](https://tokio.rs/)
- **Database:** PostgreSQL 16 via [SQLx](https://github.com/launchbadge/sqlx) 0.8
- **Cache:** Redis 7
- **Auth:** JWT (jsonwebtoken), Argon2 password hashing
- **Email:** lettre + Tera templates + AWS SES
- **Storage:** AWS S3

## Quick Start

See the [project README](../README.md) for infrastructure setup.

```sh
# Run migrations
cargo db-dev-run

# Start development server
cargo run --bin oxideauth
# or with hot-reload:
cargo watch -x "run --bin oxideauth"
```

## Architecture

```
src/
  web/          Axum handlers, middleware, DTOs, routes
  core/         Business logic, services, models, traits
  store/        PostgreSQL data access (SQLx + SeaQuery)
  cache/        Redis caching layer
  dev/          Dev tooling (DB init, fixtures)
  utils/        Shared utilities
  macros/       Internal convenience macros
```

## API Endpoints

See [docs/api_philosophy.md](../docs/api_philosophy.md) for API design principles.

## Testing

```sh
# Unit tests
cargo test --lib

# All tests (requires Postgres + Redis)
cargo test -- --test-threads=1
```
