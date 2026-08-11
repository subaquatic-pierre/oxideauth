-- migrations/03_project.sql
-- Purpose: Define projects as child entities within a workspace. Projects act as
--   logical containers for resources, tasks, memberships, and roles scoped below a workspace.
-- Notes:
--   - Each project belongs to exactly one workspace (`workspace_id` FK).
--   - `name` is human-readable; uniqueness is enforced per workspace.
--   - `code` is an optional short identifier (slug-like) and can be made unique within
--     a workspace if standardized (see TODOs).
--   - `config` is a JSONB object for per-project settings (feature flags, metadata).
--   - `created_by` / `updated_by` are audit fields; FK constraints can be added later
--     to avoid bootstrap issues (see TODOs).
CREATE TABLE IF NOT EXISTS
  project (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
    -- Parent workspace (cascades on delete to clean up child projects automatically)
    workspace_id UUID NOT NULL,
    -- Project identity
    name TEXT NOT NULL,
    -- Optional short identifier (slug/code). Uniqueness per-workspace can be enforced separately.
    code TEXT,
    description TEXT,
    owner UUID NOT NULL,
    -- Config blob for project-specific structured settings (validated below as JSON object)
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- START Meta & Tags
    -- Lightweight labels for search/segments
    tags TEXT[] NOT NULL DEFAULT '{}',
    -- Freeform structured metadata; validated as JSON object
    meta JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- END Meta & Tags
    -- START Audit
    -- Who created this row and when. `created_by` is NOT NULL to maintain audit trail.
    created_by UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Who last updated this row and when. `updated_by` may be NULL if never updated.
    updated_by UUID,
    updated_at TIMESTAMPTZ,
    -- Flexible audit payload for origin/IP/UA/etc; enforced to be a JSON object.
    audit JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- END Audit
    -- ---------- Constraints ----------
    CONSTRAINT project_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspace (id) ON UPDATE CASCADE ON DELETE CASCADE,
    -- Enforce JSON object shape
    CONSTRAINT project_audit_is_object CHECK (jsonb_typeof(audit) = 'object'),
    CONSTRAINT project_config_is_object CHECK (jsonb_typeof(config) = 'object')
    -- (FKs for created_by/updated_by → account(id) can be added in later migration;
    --  avoid circular bootstrap problems initially.)
  );

-- =========================
-- Indexes
-- =========================
-- Ensure per-workspace uniqueness of project names
CREATE UNIQUE INDEX IF NOT EXISTS project_workspace_name_key ON project (workspace_id, name);

-- Optional: enforce per-workspace uniqueness of code
-- CREATE UNIQUE INDEX IF NOT EXISTS project_workspace_code_key
--   ON project (workspace_id, code);