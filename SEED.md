# Seed Data Reference

This document lists all data created by the seed system. There are two layers:

- **Core seed** — created by `seed_all()` on every dev/test database init. Always present.
- **Test seed** — created by `seed_test_data()` optionally. Callers decide whether to include it.

---

## Core Seed (`seed_all`)

### Workspaces

| Name | Slug | Public | Owner | Description |
|---|---|---|---|---|
| System Workspace | `system` | `false` | `system` account | System workspace for root operations |
| Default Workspace | `default` | `false` | `owner` account | Default workspace for general access |

### Accounts

| Name | Email | Password | Notes |
|---|---|---|---|
| `system` | `system@system.local` | *(none)* | Internal system account — no credential, no login |
| `Owner Account` | `owner@system.local` | `ownerpass` | Default owner, configured via env vars |

> **Env overrides:** `OWNER_EMAIL`, `OWNER_PASSWORD`, `OWNER_NAME` can override the owner account defaults.

### Credentials

| Account | Workspace | Kind | Provider |
|---|---|---|---|
| `Owner Account` | System Workspace | Password | Local |

### Memberships

| Account | Workspace | Role |
|---|---|---|
| `system` | System Workspace (`system`) | `WorkspaceAdmin` |
| `Owner Account` | System Workspace (`system`) | `WorkspaceAdmin` |
| `Owner Account` | Default Workspace (`default`) | `WorkspaceAdmin` |

### Default Workspace Roles (auto-created per workspace)

| Role Name | Description |
|---|---|
| `WorkspaceAdmin` | Full administrative access within the workspace |
| `WorkspaceViewer` | Read-only access within the workspace |

---

## Test Seed (`seed_test_data`)

### Workspaces

| Name | Slug | Public | Owner | Description |
|---|---|---|---|---|
| Test Workspace | `test` | `false` | `Test Account` | Private test workspace for development |
| Public Test Workspace | `public-test` | `true` | `Public Admin` | Public test workspace — anyone can browse |
| Private Test Workspace | `private-test` | `false` | `Private Admin` | Private test workspace — members only |

### Accounts

| Name | Email | Password | Notes |
|---|---|---|---|
| `Test Account` | `test@example.com` | `testpass` | Owner + admin of `test` workspace |
| `Public Admin` | `public-admin@example.com` | `adminpass` | Owner + admin of `public-test` workspace |
| `Private Admin` | `private-admin@example.com` | `adminpass` | Owner + admin of `private-test` workspace |
| `Workspace Member` | `member@example.com` | `memberpass` | Cross-workspace viewer |

### Credentials

| Account | Credential Workspace | Kind | Provider |
|---|---|---|---|
| `Test Account` | Test Workspace (`test`) | Password | Local |
| `Public Admin` | Public Test Workspace (`public-test`) | Password | Local |
| `Private Admin` | Private Test Workspace (`private-test`) | Password | Local |
| `Workspace Member` | Test Workspace (`test`) | Password | Local |

### Memberships

| Account | Workspace | Role |
|---|---|---|
| `Test Account` | Test Workspace (`test`) | `WorkspaceAdmin` |
| `Public Admin` | Public Test Workspace (`public-test`) | `WorkspaceAdmin` |
| `Private Admin` | Private Test Workspace (`private-test`) | `WorkspaceAdmin` |
| `Workspace Member` | Test Workspace (`test`) | `WorkspaceViewer` |
| `Workspace Member` | Public Test Workspace (`public-test`) | `WorkspaceViewer` |
| `Workspace Member` | Private Test Workspace (`private-test`) | `WorkspaceViewer` |

---

## Quick Reference — All Credentials

| Slug | Email | Password | Role in that workspace |
|---|---|---|---|
| `system` | `system@system.local` | — *(no login)* | `WorkspaceAdmin` |
| `default` | `owner@system.local` | `ownerpass` | `WorkspaceAdmin` |
| `test` | `test@example.com` | `testpass` | `WorkspaceAdmin` |
| `test` | `member@example.com` | `memberpass` | `WorkspaceViewer` |
| `public-test` | `public-admin@example.com` | `adminpass` | `WorkspaceAdmin` |
| `public-test` | `member@example.com` | `memberpass` | `WorkspaceViewer` |
| `private-test` | `private-admin@example.com` | `adminpass` | `WorkspaceAdmin` |
| `private-test` | `member@example.com` | `memberpass` | `WorkspaceViewer` |

> **Note:** `owner@system.local` is also admin of the `system` workspace.
