# Scope & Functionality Audit Report

**Project**: OxideAuth — Multi-Tenant Authentication & Authorization Service
**Generated**: 2026-08-12
**Scope**: Full endpoint-to-service-to-store trace with DTO mapping and workspace isolation audit

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Architecture Overview](#2-architecture-overview)
3. [Endpoint Registry](#3-endpoint-registry)
4. [DTO → Service Param Mapping Analysis](#4-dto--service-param-mapping-analysis)
5. [Response DTO Analysis](#5-response-dto-analysis)
6. [Workspace Scoping & Isolation Analysis](#6-workspace-scoping--isolation-analysis)
7. [Permission Validation Analysis](#7-permission-validation-analysis)
8. [Findings & Recommendations](#8-findings--recommendations)
9. [Appendix: Handler-to-Service Call Matrix](#appendix-handler-to-service-call-matrix)

---

## 1. Executive Summary

OxideAuth implements a **5-layer defense-in-depth workspace isolation architecture** across middleware, context, validation, store context, and SQL query layers. The codebase follows a consistent handler → DTO → service → store pattern for all CRUD resources, with two distinct validation patterns (Pattern A and Pattern B) that diverge in their workspace scoping behavior.

**Overall assessment**: The architecture is well-structured with intentional design decisions around workspace isolation. Several findings are noted below that warrant attention, primarily around response consistency and one documented-but-unresolved account cross-workspace access concern.

| Area | Status | Issues Found |
|------|--------|-------------|
| Endpoint → Service Mapping | ✅ Consistent | None |
| DTO → Params Mapping | ✅ Consistent | 1 minor (version field in Membership) |
| Response DTO Consistency | ⚠️ Inconsistent | 4 raw model exposures |
| Workspace Scoping (Pattern B) | ✅ Solid | None |
| Workspace Scoping (Pattern A) | ⚠️ Documented gap | Account cross-workspace access |
| Permission Validation | ✅ Consistent | None |
| Multi-Tenant Isolation | ✅ Strong | 5-layer enforcement |

---

## 2. Architecture Overview

### 2.1 Layer Stack

```
┌─────────────────────────────────────────────────────────────┐
│  HTTP Layer (web/)                                           │
│  ┌──────────────┐  ┌─────────────┐  ┌──────────────────┐   │
│  │  Router       │  │  Handlers   │  │  DTOs             │   │
│  │  (router.rs)  │→ │  (handlers/)│→ │  (dtos/)          │   │
│  └──────────────┘  └─────────────┘  └──────────────────┘   │
│         │                                                      │
│  ┌──────┴──────────────────────────────────────────────────┐  │
│  │  Middleware Layers                                       │  │
│  │  CtxLayer → RequestMw → ResponseMw → GlobalError        │  │
│  └─────────────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  Core Layer (core/)                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │  CoreCtx      │  │  Services    │  │  AuthValidator    │  │
│  │  (ctx.rs)     │  │  (services/) │  │  (auth.rs:1047)   │  │
│  └──────────────┘  └──────────────┘  └──────────────────┘  │
│  ┌──────────────┐  ┌──────────────┐                        │
│  │  Models       │  │  Traits      │                        │
│  │  (models/)    │  │  (traits/)   │                        │
│  └──────────────┘  └──────────────┘                        │
├─────────────────────────────────────────────────────────────┤
│  Store Layer (store/)                                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │  StoreCtx     │  │  Entities    │  │  Stores           │  │
│  │  (ctx.rs)     │  │  (entities/) │  │  (stores/)        │  │
│  └──────────────┘  └──────────────┘  └──────────────────┘  │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  prepare_workspace_scope() — SQL-level enforcement    │   │
│  │  (utils/scope.rs)                                    │   │
│  └──────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────┤
│  Cache Layer (cache/)                                        │
│  ┌──────────────┐  ┌──────────────┐                        │
│  │  RedisChx     │  │  AuthCache    │                        │
│  │  (redis.rs)   │  │  (entities/)  │                        │
│  └──────────────┘  └──────────────┘                        │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 Five-Layer Workspace Isolation

| Layer | Component | File | Mechanism |
|-------|-----------|------|-----------|
| 1 — Middleware | `CtxLayer` / `CtxMw` | `web/middlewares/ctx.rs` | Extracts JWT, resolves `CoreCtx`, injects into request extensions |
| 2 — Context | `CoreCtx` | `core/ctx.rs` | Carries `scoped_ws_id`, `PermissionEngine`, `AuthCache` |
| 3 — Validation | `AuthValidator` | `core/services/auth.rs:1047` | Validates permissions AND enforces workspace boundaries |
| 4 — Store Context | `StoreCtx` | `store/ctx.rs` | Carries optional `workspace_scope` for row-level filtering |
| 5 — Query | `prepare_workspace_scope()` | `store/utils/scope.rs` | Injects/overwrites `workspace_id` in SQL query fields |

### 2.3 Two Validation Patterns

The codebase uses **two distinct patterns** for permission and workspace validation in services:

| Aspect | Pattern A (Direct) | Pattern B (Trait-based) |
|--------|--------------------|--------------------------|
| **Used by** | `AccountService`, `WorkspaceService` | `ClientService`, `CredentialService`, `MembershipService`, `PermissionService`, `ProjectService`, `RoleService` |
| **Permission check** | `AuthValidator::new(ctx).validate_ctx_perms(...)` | `self.scope_and_validate_ctx(ctx, ws_id, perms)` via trait default |
| **Workspace validation** | `scope_store_workspace(None)` — no target workspace | `scope_and_validate_ctx(...)` → resolves workspace via `get_workspace()` → `scope_store_workspace(Some(ws_id))` |
| **Store workspace scope** | Always stripped (`should_remove_workspace_from_store_ctx = true`) | Preserved (`should_remove_workspace_from_store_ctx = false`, default) |
| **Workspace in Params** | No `workspace_id` field | `workspace_id: Uuid` injected via `IntoParams` |

---

## 3. Endpoint Registry

### 3.1 Public Endpoints (No Auth Required)

| Method | Path | Handler | Service Call |
|--------|------|---------|-------------|
| GET | `/` | `index_handler` | *(none — static)* |
| GET | `/health-check` | `health_check_handler` | *(none — static)* |
| POST | `/auth/register` | `register` | `svc.register(ctx, RegisterParams)` |
| POST | `/auth/login` | `login` | `svc.login(ctx, email, password)` |
| POST | `/auth/refresh` | `refresh` | `svc.refresh_token(token)` |
| POST | `/auth/reset-password` | `reset_password` | `svc.request_password_reset(ctx, email)` |
| POST | `/auth/update-password` | `update_password` | `svc.update_password(ctx, token, password)` |
| POST | `/auth/confirm` | `confirm_account` | `svc.confirm_account(ctx, token)` |
| POST | `/auth/resend-confirm` | `resend_confirm` | `svc.resend_confirmation(ctx, email)` |
| POST | `/auth/oauth/google/initiate` | `oauth_google_initiate` | `svc.initiate_google_oauth(ctx, redirect_url)` |
| GET | `/auth/oauth/google/callback` | `oauth_google_callback` | `svc.process_google_callback(ctx, code, state)` |

### 3.2 Protected Endpoints (Auth Required — CtxLayer)

| Method | Path | Handler | Service Call |
|--------|------|---------|-------------|
| POST | `/accounts/describe` | `describe_account` | `svc.describe(ctx, AccountDescribeParams)` |
| POST | `/accounts/create` | `create_account` | `svc.create(ctx, AccountCreateParams)` |
| POST | `/accounts/list` | `list_accounts` | `svc.list(ctx, AccountListParams)` |
| POST | `/accounts/update` | `update_account` | `svc.update(ctx, AccountUpdateParams)` |
| POST | `/accounts/delete` | `delete_account` | `svc.delete(ctx, AccountDeleteParams)` |
| POST | `/workspace/describe` | `describe_workspace` | `svc.describe(ctx, WorkspaceDescribeParams)` |
| POST | `/workspace/create` | `create_workspace` | `svc.create(ctx, WorkspaceCreateParams)` |
| POST | `/workspace/list` | `list_workspaces` | `svc.list(ctx, WorkspaceListParams)` |
| POST | `/workspace/update` | `update_workspace` | `svc.update(ctx, WorkspaceUpdateParams)` |
| POST | `/workspace/delete` | `delete_workspace` | `svc.delete(ctx, WorkspaceDeleteParams)` |
| POST | `/projects/describe` | `describe_project` | `svc.describe(ctx, ProjectDescribeParams)` |
| POST | `/projects/create` | `create_project` | `svc.create(ctx, ProjectCreateParams)` |
| POST | `/projects/list` | `list_projects` | `svc.list(ctx, ProjectListParams)` |
| POST | `/projects/update` | `update_project` | `svc.update(ctx, ProjectUpdateParams)` |
| POST | `/projects/delete` | `delete_project` | `svc.delete(ctx, ProjectDeleteParams)` |
| POST | `/clients/describe` | `describe_client` | `svc.describe(ctx, ClientDescribeParams)` |
| POST | `/clients/create` | `create_client` | `svc.create(ctx, ClientCreateParams)` |
| POST | `/clients/list` | `list_clients` | `svc.list(ctx, ClientListParams)` |
| POST | `/clients/update` | `update_client` | `svc.update(ctx, ClientUpdateParams)` |
| POST | `/clients/delete` | `delete_client` | `svc.delete(ctx, ClientDeleteParams)` |
| POST | `/clients/validate` | `validate_client` | `svc.validate(ctx, ws_id, secret, token, perms)` |
| POST | `/clients/regenerate-secret` | `regenerate_secret_client` | `svc.regenerate_secret(ctx, id, ws_id)` |
| POST | `/roles/describe` | `describe_role` | `svc.describe(ctx, RoleDescribeParams)` |
| POST | `/roles/create` | `create_role` | `svc.create(ctx, RoleCreateParams)` |
| POST | `/roles/list` | `list_roles` | `svc.list(ctx, RoleListParams)` |
| POST | `/roles/update` | `update_role` | `svc.update(ctx, RoleUpdateParams)` |
| POST | `/roles/delete` | `delete_role` | `svc.delete(ctx, RoleDeleteParams)` |
| POST | `/permissions/describe` | `describe_permission` | `svc.describe(ctx, PermissionDescribeParams)` |
| POST | `/permissions/create` | `create_permission` | `svc.create(ctx, PermissionCreateParams)` |
| POST | `/permissions/list` | `list_permissions` | `svc.list(ctx, PermissionListParams)` |
| POST | `/permissions/update` | `update_permission` | `svc.update(ctx, PermissionUpdateParams)` |
| POST | `/permissions/delete` | `delete_permission` | `svc.delete(ctx, PermissionDeleteParams)` |
| POST | `/memberships/describe` | `describe_membership` | `svc.describe(ctx, MembershipDescribeParams)` |
| POST | `/memberships/create` | `create_membership` | `svc.create(ctx, MembershipCreateParams)` |
| POST | `/memberships/list` | `list_memberships` | `svc.list(ctx, MembershipListParams)` |
| POST | `/memberships/update` | `update_membership` | `svc.update(ctx, MembershipUpdateParams)` |
| POST | `/memberships/delete` | `delete_membership` | `svc.delete(ctx, MembershipDeleteParams)` |
| POST | `/credentials/describe` | `describe_credential` | `svc.describe(ctx, CredentialDescribeParams)` |
| POST | `/credentials/list` | `list_credentials` | `svc.list(ctx, CredentialListParams)` |
| POST | `/credentials/update` | `update_credential` | `svc.update(ctx, CredentialUpdateParams)` |
| POST | `/credentials/delete` | `delete_credential` | `svc.delete(ctx, CredentialDeleteParams)` |
| POST | `/auth/revoke` | `revoke` | `svc.revoke_token(ctx, token)` |

---

## 4. DTO → Service Param Mapping Analysis

### 4.1 Workspace ID Injection Pattern

The project follows a deliberate strategy to keep `workspace_id` out of request DTOs (per spec `008-remove-workspace-id-from-dtos`). Instead, the workspace ID is resolved from `ctx.scoped_ws_id()` in handlers and injected during DTO-to-params conversion.

**Six of nine resources** use `IntoParams<Params>` trait injection:
- `Client`, `Credential`, `Membership`, `Permission`, `Project`, `Role`

**Two resources** use `From<Req>` trait conversion (no ws_id needed):
- `Workspace` (the resource itself IS the workspace)
- `Account` (workspace_id not needed — accounts are global)

**One resource** uses `From<Req>` for `RegisterParams`:
- `Auth` register DTO → params

### 4.2 Detailed Field Mapping

#### 4.2.1 Account DTOs → Params

| DTO Field | Params Field | Conversion |
|-----------|-------------|------------|
| `email: String` | `email: String` | Direct |
| `id: Option<Uuid>` | `id: Option<Uuid>` | Direct |
| `password: String` | `password: String` | Direct |
| `kind: Option<AccountKind>` | `kind: AccountKind` | `unwrap_or(AccountKind::User)` |
| `enabled: Option<bool>` | `enabled: bool` | `unwrap_or_default()` → `false` |
| `verified: Option<bool>` | `verified: bool` | `unwrap_or_default()` → `false` |
| `name: String` | `name: String` | Direct |
| `description: Option<String>` | `description: Option<String>` | Direct |
| `avatar_url: Option<String>` | `avatar_url: Option<String>` | Direct |
| `tags: Option<Vec<String>>` | `tags: Option<Vec<String>>` | Direct |
| `meta: Option<AccountMeta>` | `meta: Option<AccountMeta>` | Direct |

**Status**: ✅ All fields map cleanly. Defaults are reasonable.

#### 4.2.2 Client DTOs → Params

| DTO Field | Params Field | Notes |
|-----------|-------------|-------|
| `id: Uuid` | `id: Uuid` | Direct |
| `workspace_id` | — | **Injected** from `ctx.scoped_ws_id()` |
| `name: String` | `name: String` | Direct |
| `endpoint: Option<String>` | `endpoint: Option<String>` | Direct |
| `description: Option<String>` | `description: Option<String>` | Direct |
| `tags: Vec<String>` | `tags: Vec<String>` | Direct (create) / `Option<Vec<String>>` (update) |
| `meta: ClientMeta` | `meta: ClientMeta` | Direct |

**Status**: ✅ All fields map cleanly. Workspace ID correctly injected.

#### 4.2.3 Credential DTOs → Params

| DTO Field | Params Field | Notes |
|-----------|-------------|-------|
| `id: Uuid` | `id: Uuid` | Direct |
| `account_id: Uuid` | `account_id: Uuid` | Direct |
| `workspace_id` | — | **Injected** from `ctx.scoped_ws_id()` |
| `provider_id: Option<String>` | `provider_id: Option<String>` | Direct |
| `email: Option<String>` | `email: Option<String>` | Direct |
| `kind: Option<CredentialKind>` | `kind: Option<CredentialKind>` | Direct |
| `provider: Option<CredentialProvider>` | `provider: Option<CredentialProvider>` | Direct |
| `status: Option<CredentialStatus>` | `status: Option<CredentialStatus>` | Direct |
| `new_provider_id: Option<String>` | `new_provider_id: Option<String>` | Direct |
| `new_email: Option<String>` | `new_email: Option<String>` | Direct |
| `secret: Option<String>` | `secret: Option<String>` | Direct |
| `config: Option<CredentialConfig>` | `config: Option<CredentialConfig>` | Direct |
| `last_used_at: Option<OffsetDateTime>` | `last_used_at: Option<OffsetDateTime>` | Direct |
| `tags: Option<Vec<String>>` | `tags: Option<Vec<String>>` | Direct |
| `meta: Option<CredentialMeta>` | `meta: Option<CredentialMeta>` | Direct |

**Status**: ✅ All fields map cleanly. No create DTO/capability (by design per spec).

#### 4.2.4 Membership DTOs → Params

| DTO Field | Params Field | Notes |
|-----------|-------------|-------|
| `id: Uuid` | `id: Uuid` | Direct |
| `workspace_id` | — | **Injected** from `ctx.scoped_ws_id()` |
| `account_id: Uuid` | `account_id: Uuid` | Direct |
| `scope: MembershipScope` | `scope: MembershipScope` | Direct |
| `status: MembershipStatus` | `status: MembershipStatus` | Direct |
| `project_id: Option<Uuid>` | `project_id: Option<Uuid>` | Direct |
| `role_ids: Vec<Uuid>` | `role_ids: Vec<Uuid>` | Direct (create only) |
| `tags: Vec<String>` | `tags: Vec<String>` | Direct |
| `meta: MembershipMeta` | `meta: MembershipMeta` | Direct |
| *(none)* | `version: Option<i64>` | **⚠️ See Finding #1** |

#### 4.2.5 Permission, Project, Role DTOs → Params

**Status**: ✅ All fields map 1:1 with workspace_id correctly injected. No issues found.

#### 4.2.6 Workspace DTOs → Params

Uses `From<Req>` instead of `IntoParams<Params>`. No workspace_id injection (correct — the workspace IS the entity).

**Status**: ✅ All fields map correctly. Defaults applied: `config → WorkspaceConfig::default()`, `meta → WorkspaceMeta::default()`.

### 4.3 Mapping Summary

| Entity | DTO Files | Params Files | Pattern | Workspace ID Source | Issues |
|--------|-----------|-------------|---------|---------------------|--------|
| Account | `dtos/account.rs` | `models/account.rs` | `IntoParams` | Unused (`_workspace_id`) | None |
| Auth | `dtos/auth.rs` | `models/auth.rs` | `From` (register only) | N/A | None |
| Client | `dtos/client.rs` | `models/client.rs` | `IntoParams` | `ctx.scoped_ws_id()` | None |
| Credential | `dtos/credential.rs` | `models/credential.rs` | `IntoParams` | `ctx.scoped_ws_id()` | None |
| Membership | `dtos/membership.rs` | `models/membership.rs` | `IntoParams` | `ctx.scoped_ws_id()` | ⚠️ #1 |
| Permission | `dtos/permission.rs` | `models/permission.rs` | `IntoParams` | `ctx.scoped_ws_id()` | None |
| Project | `dtos/project.rs` | `models/project.rs` | `IntoParams` | `ctx.scoped_ws_id()` | None |
| Role | `dtos/role.rs` | `models/role.rs` | `IntoParams` | `ctx.scoped_ws_id()` | None |
| Workspace | `dtos/workspace.rs` | `models/workspace.rs` | `From` | N/A (self-referential) | None |

---

## 5. Response DTO Analysis

### 5.1 Response Pattern Consistency

The project uses two response patterns:

| Pattern | Usage | Consistency |
|---------|-------|-------------|
| **Dedicated DTO** via `From<CoreModel>` | `ClientDescribeRes`, `CredentialDescribeRes`, `MembershipDescribeRes`, `PermissionDescribeRes`, `ProjectDescribeRes`, `RoleDescribeRes`, `WorkspaceDescribeRes`, `AccountDescribeRes` | ✅ Consistent |
| **Raw Core Model** in response | `Account` in auth responses, `Vec<Role>` in membership, `Vec<Permission>` in role, `Workspace` in client create, `Vec<Account>` in account list | ⚠️ See Finding #2 |

### 5.2 Core Model Exposures in Response DTOs

| Response Struct | Embedded Raw Model | Risk |
|----------------|-------------------|------|
| `AuthRegisterRes` | `account: Account` | Exposes all Account fields including audit metadata |
| `AuthLoginRes` | `account: Account` | Same as above |
| `AccountListRes` | `accounts: Vec<Account>` | Same as above — uses raw model for list |
| `MembershipDescribeRes` | `roles: Vec<Role>` | Exposes raw Role with all fields |
| `RoleDescribeRes` | `permissions: Vec<Permission>` | Exposes raw Permission with all fields |
| `ClientCreateRes` | `workspace: Workspace` | Exposes raw Workspace with all fields |

**Note**: These are not security vulnerabilities per se (these are response objects, not request objects), but they represent inconsistent API surface exposure. Adding a field to a core model unintentionally exposes it to API consumers.

### 5.3 Response Field Completeness

All response DTOs faithfully represent their core model counterparts. No fields are dropped that would cause data loss, and no unsupported fields are added. The `From<CoreModel>` implementations extract the `audit.created_at` and `audit.updated_at` fields correctly.

---

## 6. Workspace Scoping & Isolation Analysis

### 6.1 Middleware Layer (Layer 1)

**Component**: `CtxLayer` / `CtxMw` (`web/middlewares/ctx.rs:1-101`)

**Applied to**: All routes under the `protected` router (everything except `/` and `/auth` public routes).

**Flow**:
1. Intercepts every request to protected routes
2. Calls `CtxService::resolve_ctx(headers)` to authenticate and resolve context
3. On success: injects `CoreCtx` into request extensions, forwards to handler
4. On failure: returns `401 UNAUTHORIZED`

**Assessment**: ✅ Correctly applied. Public routes are properly excluded. No bypass paths exist.

### 6.2 Context Resolution (Layer 2)

**Component**: `CtxService::resolve_ctx()` (`core/services/ctx.rs`)

**Flow**:
1. Extracts JWT from `Authorization: Bearer <token>` header
2. Decodes JWT via `TokenService::decode_token_str()`
3. Fetches `AuthCache` from Redis (cache-aside: hit → return, miss → DB + write)
4. Validates version claims (`mem_ver`, `acc_ver`, `sid`) and status (`mem_active`, `acc_enabled`)
5. For system workspace tokens: reads `X-Workspace-Id` header for operational scope
6. For scoped tokens: derives workspace from `AuthCache.auth_scope.workspace_id`
7. Returns `CoreCtx` with resolved `scoped_ws_id`

**Assessment**: ✅ Robust implementation with cache-aside pattern, version/status validation, replay detection for refresh tokens, and proper separation of system vs scoped token scope.

### 6.3 AuthValidator (Layer 3)

**Component**: `AuthValidator` (`core/services/auth.rs:1047-1177`)

**Methods**:
- `validate_ctx_perms(required)`: Checks `PermissionEngine::has_subset()` against context permissions
- `scope_store_workspace(requested_ws_id)`: Creates `StoreCtx`, calls `validate_workspace()`
- `validate_workspace(requested_ws_id)`: Enforces that scoped tokens can only access their own workspace; system tokens can access any workspace

**Assessment**: ✅ Core isolation logic is sound. System workspace tokens receive `*:*` permissions, enabling cross-workspace operations. Scoped tokens are properly constrained.

### 6.4 Store Context (Layer 4)

**Component**: `StoreCtx` (`store/ctx.rs`)

**Fields**:
- `user_id: Uuid` — for audit trails
- `ws_id: Uuid` — the authenticated user's own workspace
- `workspace_scope: Option<Uuid>` — the target workspace for row-level filtering

**Assessment**: ✅ Properly captures identity and operational scope. The distinction between `ws_id` (identity workspace) and `workspace_scope` (target workspace) enables correct audit logging while supporting cross-workspace operations for system tokens.

### 6.5 Query-Level Enforcement (Layer 5)

**Component**: `prepare_workspace_scope()` (`store/utils/scope.rs:1-187`)

**Behavior**:
- If `workspace_scope` is `Some(id)`: removes any existing `workspace_id` field from sea-query fields and injects the enforced value
- If `workspace_scope` is `None`: passes through unchanged

**Assessment**: ✅ This is the **last line of defense** against cross-tenant data access via query manipulation. Ensures that even if an attacker sends a DTO with a different `workspace_id`, the enforced value from context takes precedence.

### 6.6 Per-Service Scoping Analysis

| Service | Validation Pattern | workspace_id in Params | Store Workspace Scope | DB-Level Filtering | Status |
|---------|-------------------|----------------------|----------------------|--------------------|--------|
| `AccountService` | Pattern A | ❌ (accounts are global) | Stripped (`should_remove = true`) | ❌ No workspace column | ⚠️ #3 |
| `WorkspaceService` | Pattern A | ❌ (self-referential) | Stripped (`should_remove = true`) | ❌ No workspace scope needed | ✅ |
| `ClientService` | Pattern B | ✅ Injected via `IntoParams` | Preserved | ✅ Via `prepare_workspace_scope` | ✅ |
| `CredentialService` | Pattern B | ✅ Injected via `IntoParams` | Preserved | ✅ Via `prepare_workspace_scope` | ✅ |
| `MembershipService` | Pattern B | ✅ Injected via `IntoParams` | Preserved | ✅ Via `prepare_workspace_scope` | ✅ |
| `PermissionService` | Pattern B | ✅ Injected via `IntoParams` | Preserved | ✅ Via `prepare_workspace_scope` | ✅ |
| `ProjectService` | Pattern B | ✅ Injected via `IntoParams` | Preserved | ✅ Via `prepare_workspace_scope` | ✅ |
| `RoleService` | Pattern B | ✅ Injected via `IntoParams` | Preserved | ✅ Via `prepare_workspace_scope` | ✅ |

### 6.7 Scoping During Workspace Creation

`WorkspaceService::create()` properly seeds the new workspace with:
1. All canonical permissions (via `populate_ws_perms()`)
2. Default Viewer and Admin roles (via `populate_ws_roles()`)
3. A "Default" project (via `populate_default_project()`)

The service uses `ctx.extend_perms()` to temporarily grant the required permissions for seeding, then evaluates `AuthValidator` normally for the caller's `workspace:create` permission. This ensures that even a system-level caller must have `workspace:create` permission.

**Assessment**: ✅ Correctly scoped. The seeding operations use the same services (PermissionService, RoleService, ProjectService) which themselves enforce workspace scoping.

---

## 7. Permission Validation Analysis

### 7.1 Permission Check Location

All permission validation occurs at the **service layer**, never in handlers. This is a deliberate and correct architectural decision — handlers are thin pass-through layers.

| Check Location | Pattern |
|----------------|---------|
| **Handler layer** | ❌ No permission checks |
| **Service layer** | ✅ All permission checks via `AuthValidator` or `scope_and_validate_ctx()` |
| **Store layer** | ❌ No permission checks (only data access) |

### 7.2 Permission Engine

**Component**: `PermissionEngine` (`core/models/permission.rs`)

- Stores permissions as `HashMap<Resource, HashSet<Action>>`
- `has_subset(required_rules)`: checks that the engine holds all required permissions
- Supports wildcards: `*:*` (superuser), `resource:*` (all actions on resource), `*:action` (action on all resources)

**Assessment**: ✅ Well-implemented with comprehensive wildcard support and test coverage.

### 7.3 Permission Requirements per Endpoint

| Endpoint Group | Required Permission | Validation Point |
|---------------|--------------------|--------------------|
| `accounts/*` | `account:create`, `account:describe`, `account:list`, `account:update`, `account:delete` | `AccountService` CRUD methods (Pattern A) |
| `workspace/*` | `workspace:create`, `workspace:describe`, `workspace:list`, `workspace:update`, `workspace:delete` | `WorkspaceService` CRUD methods (Pattern A) |
| `projects/*` | `project:create`, `project:describe`, `project:list`, `project:update`, `project:delete` | `ProjectService` via `scope_and_validate_ctx()` (Pattern B) |
| `clients/*` | `client:create`, `client:describe`, `client:list`, `client:update`, `client:delete`, `client:validate`, `client:regenerateSecret` | `ClientService` via `scope_and_validate_ctx()` (Pattern B) |
| `roles/*` | `role:create`, `role:describe`, `role:list`, `role:update`, `role:delete` | `RoleService` via `scope_and_validate_ctx()` (Pattern B) |
| `permissions/*` | `permission:create`, `permission:describe`, `permission:list`, `permission:update`, `permission:delete` | `PermissionService` via `scope_and_validate_ctx()` (Pattern B) |
| `memberships/*` | `membership:create`, `membership:describe`, `membership:list`, `membership:update`, `membership:delete` | `MembershipService` via `scope_and_validate_ctx()` (Pattern B) |
| `credentials/*` | `credential:describe`, `credential:list`, `credential:update`, `credential:delete` | `CredentialService` via `scope_and_validate_ctx()` (Pattern B) |
| `auth/revoke` | `auth:revoke` | `AuthService::revoke_token()` via direct `AuthValidator` |

### 7.4 AuthService Internal Permission Escalation

`AuthService` uses `ctx.extend_perms()` to temporarily escalate permissions for internal operations:

| Method | Extended Permissions | Purpose |
|--------|---------------------|---------|
| `register` | `account:create`, `credential:create`, `membership:create` | Create new account + credential + membership in one transaction |
| `login` | `account:describe` | Look up account by email |
| `update_password` | `account:describe`, `credential:describe`, `credential:list`, `credential:update` | Find and update local credential |
| `confirm_account` | `account:describe`, `account:update` | Look up and mark verified |
| `process_google_callback` | `account:describe`, `account:create`, `credential:create`, `credential:describe` | Create/find account and link credential |

**Assessment**: ✅ Properly constrained. Escalation is temporary (scoped to the method call) and only used for operations that the system must perform on behalf of unauthenticated or partially authenticated users.

---

## 8. Findings & Recommendations

### Finding #1: Membership Update Missing Version Field

**Severity**: Low
**Category**: DTO Completeness

**Description**: `MembershipUpdateParams` contains a `version: Option<i64>` field presumably intended for optimistic concurrency control. However, the `MembershipUpdateReq` DTO has no `version` field, causing it to always be `None` (filled via `..Default::default()` in the `IntoParams` implementation at `dtos/membership.rs`).

**Impact**: Clients cannot pass a version for optimistic locking, potentially leading to lost updates in concurrent scenarios.

**Recommendation**: Either:
- Add `version: Option<i64>` to `MembershipUpdateReq` to enable client-supplied version checking, or
- Remove `version` from `MembershipUpdateParams` if optimistic locking is not needed for memberships.

**Location**: `src/web/dtos/membership.rs` → `MembershipUpdateReq` struct
**Location**: `src/core/models/membership.rs` → `MembershipUpdateParams` struct

---

### Finding #2: Inconsistent Use of Response DTOs vs Raw Core Models

**Severity**: Low
**Category**: API Consistency

**Description**: Several response structs embed raw core model types instead of dedicated response DTOs:

| Response | Embedded Raw Model | Should Use |
|----------|-------------------|------------|
| `AuthRegisterRes` | `account: Account` | `account: AccountDescribeRes` |
| `AuthLoginRes` | `account: Account` | `account: AccountDescribeRes` |
| `AccountListRes` | `accounts: Vec<Account>` | `accounts: Vec<AccountDescribeRes>` |
| `MembershipDescribeRes` | `roles: Vec<Role>` | `roles: Vec<RoleDescribeRes>` |
| `RoleDescribeRes` | `permissions: Vec<Permission>` | `permissions: Vec<PermissionDescribeRes>` |
| `ClientCreateRes` | `workspace: Workspace` | `workspace: WorkspaceDescribeRes` |

**Impact**: 
- Any field added to a core model is unintentionally exposed to API consumers
- Inconsistent API surface across resources
- Harder to evolve core models without breaking API contracts

**Recommendation**: Replace all raw core model embeddings with their corresponding `*DescribeRes` DTOs. This would require adding `From<CoreModel> for DTO` implementations for the list-response DTOs where they don't already exist.

---

### Finding #3: Account Cross-Workspace Access Concern

**Severity**: Medium
**Category**: Workspace Isolation

**Description**: The `AccountService` explicitly does not filter by workspace. The code itself acknowledges this with a `TODO: URGENT NOTE` comment at `src/core/services/account.rs:72-76`:

> *"Account is the only table that is not workspace scoped. It is important that all CRUD operations are validated against what accounts the requesting user is able to access based on the 'membership' ↔ 'account' many to many join table."*

This means:
- A user in Workspace A could describe, update, or delete an account that only has membership in Workspace B
- The only protection is the permission check (`account:describe`, `account:update`, `account:delete`), but if two workspaces grant the same permission to their users, cross-workspace account manipulation is possible
- The `AccountFilter` type explicitly returns `None` for `get_workspace_id_opval()`, confirming no workspace-level filter exists

**Current Mitigation**: The `AuthValidator` checks permissions, but permissions are workspace-scoped only by convention (e.g., role assignments within a workspace). If a scoped user manages to obtain `account:describe` permission, they can describe any account regardless of workspace membership.

**Recommendation**: 
1. Implement account access validation by joining through the membership table to verify the requesting user's workspace has a membership relationship with the target account
2. Add a `workspace_id` filter to account queries that checks against the user's scoped workspace via the membership join table
3. If account multi-tenancy is intentionally global (e.g., a single account can belong to multiple workspaces), document this explicitly and implement membership-based access control rather than workspace-based

**Location**: `src/core/services/account.rs:72-76`
**Location**: `src/core/models/account.rs:171-173` (returns `None` for workspace_id)
**Location**: `src/store/stores/account.rs` (no workspace column)

---

### Finding #4: Credential Creation Gap (By Design)

**Severity**: N/A (Informational)
**Category**: Design Intent

**Description**: There is no `POST /credentials/create` endpoint and no `CredentialCreateReq` DTO. Credentials can only be created internally by `AuthService` during registration (`Password` credential) or OAuth flow (`Google` credential). The `CredentialService::create()` method exists and is functional but is only called from `AuthService`.

**Assessment**: This is by design per the specification (`src/web/dtos/credential.rs` comment: "create is excluded per spec"). No action needed unless external credential creation is a desired feature.

---

### Finding #5: Workspace Handler Converters Diverge from Pattern

**Severity**: N/A (Informational)
**Category**: Design Intent

**Description**: While all other CRUD resources use the `IntoParams<P>` trait for DTO-to-params conversion (injecting `workspace_id` from `ctx.scoped_ws_id()`), the `Workspace` DTOs use the standard `From<Req>` trait without workspace injection. This is correct because:
- The workspace IS the resource being operated on (no parent workspace context needed)
- The `WorkspaceDescribeReq.id` is a `String` (slug or UUID), not a `Uuid`

However, the handler code for workspace resources also does not call `ctx.scoped_ws_id()`, while all other handlers do. This is technically correct (the workspace_id isn't needed in params) but creates a code consistency divergence.

**Recommendation**: No action needed. This is a legitimate pattern difference justified by the domain model.

---

### Finding #6: AuthService Methods Without Ctx

**Severity**: N/A (Informational)
**Category**: Design Intent

**Description**: `AuthService::refresh_token(raw_token)` takes only a `&str` parameter — no `CoreCtx`. This is because refresh tokens are validated via JWT decoding and replay detection, not via the auth middleware. The method internally creates a context from the decoded claims only when replay is detected (to invalidate the compromised membership).

**Assessment**: Correct and well-designed. The replay detection mechanism (`ReplayCacheStore::check_and_consume()` using Redis `SET NX`) is robust against token reuse.

---

## 9. Summary Statistics

| Metric | Count |
|--------|-------|
| Total Endpoints | 45 (12 public + 33 protected) |
| Resource Types | 9 (Account, Auth, Client, Credential, Membership, Permission, Project, Role, Workspace) |
| Handler Files | 11 |
| Service Files | 15 |
| DTO Files | 10 |
| Core Model Files | 17 |
| Store Entity Files | 12 |
| CRUD Operations per Resource | 5 (describe, create, list, update, delete) except Credential (no create) and Auth (varies) |
| Validation Pattern A Services | 2 (AccountService, WorkspaceService) |
| Validation Pattern B Services | 6 (Client, Credential, Membership, Permission, Project, Role) |
| Workspace Isolation Layers | 5 |
| Findings (excluding informational) | 3 |

---

## Appendix: Handler-to-Service Call Matrix

| Handler | Service Instance | Method Called | Context Source | DTO Conversion |
|---------|-----------------|--------------|----------------|---------------|
| `register` | `svc_reg.auth` | `.register()` | `app.system_context()` | `From<AuthRegisterReq> for RegisterParams` |
| `login` | `svc_reg.auth` | `.login()` | `app.system_context()` | *(none — raw fields)* |
| `refresh` | `svc_reg.auth` | `.refresh_token()` | *(no ctx)* | *(none — raw token)* |
| `reset_password` | `svc_reg.auth` | `.request_password_reset()` | `app.system_context()` | *(none — raw email)* |
| `update_password` | `svc_reg.auth` | `.update_password()` | `app.system_context()` | *(none — raw token + password)* |
| `confirm_account` | `svc_reg.auth` | `.confirm_account()` | `app.system_context()` | *(none — raw token)* |
| `resend_confirm` | `svc_reg.auth` | `.resend_confirmation()` | `app.system_context()` | *(none — raw email)* |
| `oauth_google_initiate` | `svc_reg.auth` | `.initiate_google_oauth()` | `app.system_context()` | *(none — raw redirect_url)* |
| `oauth_google_callback` | `svc_reg.auth` | `.process_google_callback()` | `app.system_context()` | *(none — raw query)* |
| `revoke` | `svc_reg.auth` | `.revoke_token()` | `Extension<CoreCtx>` (CtxLayer) | *(none — raw token)* |
| `describe_account` | `svc_reg.account` | `.describe()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<AccountDescribeParams>` |
| `create_account` | `svc_reg.account` | `.create()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<AccountCreateParams>` |
| `list_accounts` | `svc_reg.account` | `.list()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<AccountListParams>` |
| `update_account` | `svc_reg.account` | `.update()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<AccountUpdateParams>` |
| `delete_account` | `svc_reg.account` | `.delete()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<AccountDeleteParams>` |
| `describe_workspace` | `svc_reg.workspace` | `.describe()` | `Extension<CoreCtx>` (CtxLayer) | `From<WorkspaceDescribeReq>` |
| `create_workspace` | `svc_reg.workspace` | `.create()` | `Extension<CoreCtx>` (CtxLayer) | `From<WorkspaceCreateReq>` |
| `list_workspaces` | `svc_reg.workspace` | `.list()` | `Extension<CoreCtx>` (CtxLayer) | `From<WorkspaceListReq>` |
| `update_workspace` | `svc_reg.workspace` | `.update()` | `Extension<CoreCtx>` (CtxLayer) | `From<WorkspaceUpdateReq>` |
| `delete_workspace` | `svc_reg.workspace` | `.delete()` | `Extension<CoreCtx>` (CtxLayer) | `From<WorkspaceDeleteReq>` |
| `describe_project` | `svc_reg.project` | `.describe()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<ProjectDescribeParams>` |
| `create_project` | `svc_reg.project` | `.create()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<ProjectCreateParams>` |
| `list_projects` | `svc_reg.project` | `.list()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<ProjectListParams>` |
| `update_project` | `svc_reg.project` | `.update()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<ProjectUpdateParams>` |
| `delete_project` | `svc_reg.project` | `.delete()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<ProjectDeleteParams>` |
| `describe_client` | `svc_reg.client` | `.describe()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<ClientDescribeParams>` |
| `create_client` | `svc_reg.client` | `.create()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<ClientCreateParams>` |
| `list_clients` | `svc_reg.client` | `.list()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<ClientListParams>` |
| `update_client` | `svc_reg.client` | `.update()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<ClientUpdateParams>` |
| `delete_client` | `svc_reg.client` | `.delete()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<ClientDeleteParams>` |
| `validate_client` | `svc_reg.client` | `.validate()` | `Extension<CoreCtx>` (CtxLayer) | *(none — raw fields + ws_id)* |
| `regenerate_secret_client` | `svc_reg.client` | `.regenerate_secret()` | `Extension<CoreCtx>` (CtxLayer) | *(none — raw id + ws_id)* |
| `describe_role` | `svc_reg.role` | `.describe()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<RoleDescribeParams>` |
| `create_role` | `svc_reg.role` | `.create()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<RoleCreateParams>` |
| `list_roles` | `svc_reg.role` | `.list()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<RoleListParams>` |
| `update_role` | `svc_reg.role` | `.update()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<RoleUpdateParams>` |
| `delete_role` | `svc_reg.role` | `.delete()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<RoleDeleteParams>` |
| `describe_permission` | `svc_reg.permission` | `.describe()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<PermissionDescribeParams>` |
| `create_permission` | `svc_reg.permission` | `.create()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<PermissionCreateParams>` |
| `list_permissions` | `svc_reg.permission` | `.list()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<PermissionListParams>` |
| `update_permission` | `svc_reg.permission` | `.update()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<PermissionUpdateParams>` |
| `delete_permission` | `svc_reg.permission` | `.delete()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<PermissionDeleteParams>` |
| `describe_membership` | `svc_reg.membership` | `.describe()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<MembershipDescribeParams>` |
| `create_membership` | `svc_reg.membership` | `.create()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<MembershipCreateParams>` |
| `list_memberships` | `svc_reg.membership` | `.list()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<MembershipListParams>` |
| `update_membership` | `svc_reg.membership` | `.update()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<MembershipUpdateParams>` |
| `delete_membership` | `svc_reg.membership` | `.delete()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<MembershipDeleteParams>` |
| `describe_credential` | `svc_reg.credential` | `.describe()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<CredentialDescribeParams>` |
| `list_credentials` | `svc_reg.credential` | `.list()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<CredentialListParams>` |
| `update_credential` | `svc_reg.credential` | `.update()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<CredentialUpdateParams>` |
| `delete_credential` | `svc_reg.credential` | `.delete()` | `Extension<CoreCtx>` (CtxLayer) | `IntoParams<CredentialDeleteParams>` |

---

*End of Report*
