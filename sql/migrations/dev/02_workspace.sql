-- migrations/02_workspace.sql
-- Purpose: Define multi-tenant containers ("workspaces") that isolate projects, memberships,
--   and resources under a common scope. A workspace acts as the root boundary for tenant data.
-- Notes:
--   - `name` is human-readable; `slug` is a URL-safe unique identifier (e.g. used in routing).
--   - `config` is a JSONB object for per-workspace settings (feature flags, limits, branding).
--   - `created_by` / `updated_by` are audit fields; FK constraints can be added later to avoid
--     bootstrap problems (see TODOs).
CREATE TABLE IF NOT EXISTS
  workspace (
    -- Primary key for workspace (globally unique).
    id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
    -- Identity fields
    -- `name` = display label; `slug` = unique URL-safe identifier (enforced unique below).
    name TEXT UNIQUE NOT NULL,
    slug TEXT UNIQUE NOT NULL,
    description TEXT,
    owner UUID NOT NULL,
    -- Config blob for per-workspace structured settings (JSON object enforced below).
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- START Meta & Tags
    -- Lightweight labels for search/segments.
    tags TEXT[] NOT NULL DEFAULT '{}',
    -- Freeform structured metadata; validated as JSON object.
    meta JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- END Meta & Tags
    -- START Audit
    -- Who created this row and when. `created_by` is NOT NULL to maintain a full audit trail.
    created_by UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Who last updated this row and when. `updated_by` may be NULL if never updated.
    updated_by UUID,
    updated_at TIMESTAMPTZ,
    -- Flexible audit payload for origin/IP/UA/etc; enforced to be a JSON object.
    audit JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- END Audit
    -- Constraints: enforce JSON object shape where expected.
    CONSTRAINT workspace_audit_is_object CHECK (jsonb_typeof(audit) = 'object'),
    CONSTRAINT workspace_config_is_object CHECK (jsonb_typeof(config) = 'object')
    -- (Foreign keys for created_by/updated_by → account(id) can be added in a later migration
    --  to avoid circular bootstrapping; see TODOs.)
  );

-- =========================
-- Indexes
-- =========================
-- Optional: index slug for fast lookup (besides uniqueness already enforced).
CREATE INDEX IF NOT EXISTS idx_workspace_slug ON workspace (slug);