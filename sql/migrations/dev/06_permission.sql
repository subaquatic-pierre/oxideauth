-- migrations/06_permission.sql
-- Purpose: Define fine-grained permissions within a workspace. Permissions are
--   typically bound to roles to control access to resources and actions.
-- Notes:
--   - Each permission belongs to exactly one workspace (global or tenant).
--   - `name` should be unique per workspace (e.g. 'project.read','project.write').
--   - `code` is optional as a short identifier if needed for programmatic usage.
--   - `meta` and `tags` support lightweight extension and search.
--   - `created_by` / `updated_by` are audit fields; FKs can be added later if bootstrap-safe.
CREATE TABLE IF NOT EXISTS
  permission (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
    -- Scope: permissions are defined per-workspace
    workspace_id UUID NOT NULL,
    -- Permission identity
    name TEXT NOT NULL, -- canonical identifier, e.g. 'project.read'
    code TEXT, -- optional short key
    description TEXT, -- human-readable description
    -- START Meta & Tags
    tags TEXT[] NOT NULL DEFAULT '{}', -- lightweight labels
    meta JSONB NOT NULL DEFAULT '{}'::jsonb, -- structured metadata
    -- END Meta & Tags
    -- START Audit
    created_by UUID NOT NULL, -- who created this permission
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by UUID, -- last updater (nullable)
    updated_at TIMESTAMPTZ,
    audit JSONB NOT NULL DEFAULT '{}'::jsonb, -- flexible audit payload
    -- END Audit
    -- ---------- Constraints ----------
    CONSTRAINT perm_workspace_fk FOREIGN KEY (workspace_id) REFERENCES workspace (id) ON UPDATE CASCADE ON DELETE CASCADE,
    -- Enforce JSON object shape
    CONSTRAINT perm_meta_is_object CHECK (jsonb_typeof(meta) = 'object'),
    CONSTRAINT perm_audit_is_object CHECK (jsonb_typeof(audit) = 'object'),
    -- Ensure per-workspace uniqueness of permission names
    CONSTRAINT permission_workspace_name_key UNIQUE (workspace_id, name)
    -- Optional: enforce uniqueness of code per workspace
    -- , CONSTRAINT permission_workspace_code_key UNIQUE (workspace_id, code)
  );

-- =========================
-- Indexes
-- =========================
-- UNIQUE(workspace_id, name) already provides fast lookups.
-- Optionally enforce and index (workspace_id, code) if used heavily.
-- CREATE UNIQUE INDEX IF NOT EXISTS permission_workspace_code_key
--   ON permission(workspace_id, code);
-- Optional: add GIN indexes if tags/meta will be queried often.
-- CREATE INDEX IF NOT EXISTS idx_permission_tags_gin ON permission USING GIN(tags);
-- CREATE INDEX IF NOT EXISTS idx_permission_meta_gin ON permission USING GIN(meta);