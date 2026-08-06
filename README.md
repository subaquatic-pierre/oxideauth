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

All 39 endpoints use JSON POST with a standard `{ success, status, data }` response envelope.

| Resource | Endpoints | Description |
|----------|-----------|-------------|
| Health | 2 | `GET /`, `GET /health-check` |
| Workspaces | 5 | CRUD + List — multi-tenant containers |
| Accounts | 5 | CRUD + List — user identity management |
| Projects | 5 | CRUD + List — scoped work areas |
| Roles | 5 | CRUD + List — permission bundles |
| Permissions | 5 | CRUD + List — fine-grained access control |
| Memberships | 5 | CRUD + List — account-to-workspace links |
| Credentials | 4 | Describe, List, Update, Delete |
| Tokens | 3 | Describe, List, Delete — token blacklist |

## Documentation

Full API reference, concept guides, and architecture docs are in the [project docs](../docs/).

## Postman Collection

Import-ready collection with all 39 endpoints, example payloads, auto-populated variables, and test scripts:

[`references/OxideAuth.postman_collection.json`](references/OxideAuth.postman_collection.json)

1. Import into Postman
2. Set the `host` variable (default: `http://127.0.0.1:8000`)
3. Set the `token` variable to a valid Bearer JWT
4. Run **Workspace → Create Workspace** first to populate the `{{workspace_id}}` variable

## Testing

```sh
# Unit tests
cargo test --lib

# All tests (requires Postgres + Redis)
cargo test -- --test-threads=1
```
