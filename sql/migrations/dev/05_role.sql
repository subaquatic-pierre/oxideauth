-- migrations/05_role.sql
-- Purpose: Define roles (permission bundles) within a workspace. Roles serve as
--   reusable collections of capabilities that can be attached to memberships.
-- Notes:
--   - Each role belongs to exactly one workspace (including the global workspace).
--   - `name` is human-readable (e.g. 'owner','editor','viewer') and unique within a workspace.
--   - `meta` and `tags` support lightweight extension and search.
--   - `created_by` / `updated_by` are audit fields; FKs can be added later if bootstrap-safe.
CREATE TABLE IF NOT EXISTS
  role (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
    -- Scope: roles are defined per-workspace (global or tenant-specific)
    workspace_id UUID NOT NULL,
    -- Role identity
    name TEXT NOT NULL, -- short identifier like 'owner','editor','viewer'
    description TEXT, -- optional human-friendly description
    -- START Meta & Tags
    tags TEXT[] NOT NULL DEFAULT '{}', -- lightweight labels for grouping/search
    meta JSONB NOT NULL DEFAULT '{}'::jsonb, -- structured metadata, enforced as JSON object
    -- END Meta & Tags
    -- START Audit
    created_by UUID NOT NULL, -- who created this role
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by UUID, -- last updater (nullable)
    updated_at TIMESTAMPTZ,
    audit JSONB NOT NULL DEFAULT '{}'::jsonb, -- flexible audit payload (IP/UA/etc.)
    -- END Audit
    -- ---------- Constraints ----------
    CONSTRAINT role_workspace_fk FOREIGN KEY (workspace_id) REFERENCES workspace (id) ON UPDATE CASCADE ON DELETE CASCADE,
    -- Enforce JSON object shape
    CONSTRAINT role_meta_is_object CHECK (jsonb_typeof(meta) = 'object'),
    CONSTRAINT role_audit_is_object CHECK (jsonb_typeof(audit) = 'object'),
    -- Ensure role name uniqueness within a workspace
    CONSTRAINT role_workspace_name_key UNIQUE (workspace_id, name)
    -- (FKs for created_by/updated_by → account(id) can be added in later migration
    --  once bootstrap order is established.)
  );

-- =========================
-- Indexes
-- =========================
-- The UNIQUE(workspace_id, name) constraint already provides fast lookups.
-- Optionally add GIN indexes if tags/meta will be queried often.
-- CREATE INDEX IF NOT EXISTS idx_role_tags_gin ON role USING GIN(tags);
-- CREATE INDEX IF NOT EXISTS idx_role_meta_gin ON role USING GIN(meta);