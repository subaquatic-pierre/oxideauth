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

All 50 endpoints use JSON POST with a standard `{ success, status, data }` response envelope (except 2 GET health endpoints and 1 GET OAuth callback).

| Resource | Endpoints | Description |
|----------|-----------|-------------|
| Health | 2 | `GET /`, `GET /health-check` |
| Auth | 11 | Register, Login, Refresh, OAuth2, password & token management |
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

Import-ready collection with all 50 endpoints, example payloads, auto-populated variables, and test scripts:

[`references/OxideAuth.postman_collection.json`](references/OxideAuth.postman_collection.json)

1. Import into Postman
2. Set the `host` variable (default: `http://127.0.0.1:6000`)
3. Set the `token` variable to a valid Bearer JWT
4. Run **Workspace → Create Workspace** first to populate the `{{workspace_id}}` variable

## Deployment

The API uses **build-only** verification on tag push — it builds and verifies compilation but does not deploy to a production environment (no deployment target configured yet).

### Option 1: Git Tag Push (CI Trigger)

```sh
# From the api/ directory, create and push a semantic version tag:
git tag 1.0.0
git push origin 1.0.0
```

Pushing a tag matching `*.*.*` triggers the GitHub Actions workflow at `.github/workflows/deploy.yml`, which runs `cargo build --release` to verify compilation. Unit tests are excluded (they require a database connection).

### Option 2: Manual Deploy via Script

```sh
make deploy patch    # bump patch version (0.0.0 → 0.0.1)
make deploy minor    # bump minor version (0.0.0 → 0.1.0)
make deploy major    # bump major version (0.0.0 → 1.0.0)
make deploy 1.2.3    # use explicit version
```

The `make deploy` command runs `scripts/deploy.sh`, which:
1. Bumps the version in `Cargo.toml`
2. Runs `cargo build --release`
3. Creates and pushes an `X.Y.Z` tag
4. Commits and pushes version bump changes

All tags use semantic versioning (e.g., `1.2.3`). Deployments target the `main` branch.

## Testing

```sh
# Unit tests
cargo test --lib

# All tests (requires Postgres + Redis)
cargo test -- --test-threads=1
```
