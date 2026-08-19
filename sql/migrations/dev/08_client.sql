-- migrations/14_client.sql
-- Purpose: Define registered microservice clients within a workspace. Clients represent
--   external services that consume the auth service for token validation and receive
--   push notifications for authorization state changes.
-- Notes:
--   - Each client belongs to exactly one workspace.
--   - `name` is unique within a workspace.
--   - `secret_hash` stores the hashed client secret (never exposed after creation).
--   - `endpoint` is an optional URL where push notifications are delivered.
--   - `meta` and `tags` support lightweight extension and search.
--   - `created_by` / `updated_by` are audit fields; FKs can be added later if bootstrap-safe.
CREATE TABLE IF NOT EXISTS
  client (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
    -- Scope: clients are defined per-workspace
    workspace_id UUID NOT NULL,
    -- Client identity
    name TEXT NOT NULL,
    -- Push notification endpoint (optional — clients without an endpoint do not receive pushes)
    endpoint TEXT,
    description TEXT,
    -- START Meta & Tags
    tags TEXT[] NOT NULL DEFAULT '{}',
    meta JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- END Meta & Tags
    -- START Audit
    created_by UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by UUID,
    updated_at TIMESTAMPTZ,
    audit JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- END Audit
    -- ---------- Constraints ----------
    CONSTRAINT client_workspace_fk FOREIGN KEY (workspace_id) REFERENCES workspace (id) ON UPDATE CASCADE ON DELETE CASCADE,
    -- Enforce JSON object shape
    CONSTRAINT client_meta_is_object CHECK (jsonb_typeof(meta) = 'object'),
    CONSTRAINT client_audit_is_object CHECK (jsonb_typeof(audit) = 'object'),
    -- Ensure client name uniqueness within a workspace
    CONSTRAINT client_workspace_name_key UNIQUE (workspace_id, name)
    -- (FKs for created_by/updated_by → account(id) can be added in later migration
    --  once bootstrap order is established.)
  );

-- =========================
-- Indexes
-- =========================
-- The UNIQUE(workspace_id, name) constraint already provides fast lookups per workspace.
-- Add an explicit index on workspace_id for foreign-key lookups and workspace-scoped list queries.
CREATE INDEX IF NOT EXISTS idx_client_workspace_id ON client (workspace_id);