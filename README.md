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
2. Set the `host` variable (default: `http://127.0.0.1:8000`)
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

Pushing a tag matching `*.*.*` triggers the GitHub Actions workflow at `.github/workflows/deploy.yml`, which runs `cargo build --release` to verify compilation. Tests are excluded from the deploy build (the integration suite requires a live database).

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

The test suite is split into two tiers, separated by the `integration` cargo feature.

### Unit tests — mocked, no database or Redis

Business logic (services, stores, models) is tested against in-memory fakes, so the entire logic layer runs with no infrastructure:

- `store::dbx::MockDbx` — a configurable, safe in-memory `DbExecutor`. Register canned responses with `.with_one::<T>()`, `.with_optional::<T>()`, `.with_all::<T>()`, `.with_execute()` (served FIFO per row type).
- `cache::mock::MockChx` — an in-memory `CacheExecutor` with real read / write / `incr` / delete semantics.

```sh
# from api/:
cargo test --lib
# or from the repo root:
cargo test -p oxideauth --lib
```

### Integration tests — real Postgres + Redis

The only tests that touch a real database are the SQL **query** tests and the DB seed/migration helpers, because the query layer (`store/queries/*`) is the sole place SQL is built and executed:

- `tests/queries/{batch,contains,count,crud,join}.rs` — the SQL query tests (moved out of the library).
- `src/dev/db.rs` — reset / migrate / seed helpers.

These are gated behind the `integration` feature (declared in `api/Cargo.toml`) and require a running Postgres + Redis. They use the `Config::test_config()` URLs (`postgres://test_user:password@localhost:5432/test_db`, `redis://127.0.0.1:6379`) and migrate + seed the database once via `dev::init::init_test()`.

```sh
# Full suite — unit + integration (requires Postgres + Redis):
cargo test -p oxideauth --features integration

# Compile-check the integration suite without running it:
cargo check -p oxideauth --tests --features integration
```

### Structure

```
api/
  src/                    inline #[cfg(test)] mod tests — unit tests (mocked)
  tests/
    main.rs               #[cfg(feature = "integration")] mod queries;  ← gates the integration suite
    queries/
      mod.rs              declares the query test modules
      batch.rs            store::queries::batch
      contains.rs         store::queries::contains
      count.rs            store::queries::count
      crud.rs             store::queries::crud
      join.rs             store::queries::join
```

### Running a single test

```sh
# A single unit test (mocked, no DB):
cargo test -p oxideauth --lib store::stores::account::tests::test_create_get_ok

# A single query integration test (requires Postgres + Redis):
cargo test -p oxideauth --features integration --test main queries::crud::test_create_and_get_pass
```
