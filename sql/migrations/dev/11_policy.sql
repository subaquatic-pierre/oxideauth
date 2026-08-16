-- migrations/11_policy.sql
-- Purpose: Define workspace-scoped authorization policies. A policy is an
--   AWS-like rule (effect + actions + resource + optional constraint) that is
--   attached to roles and/or memberships and compiled at write time into a
--   runtime key for O(1) lookup.
-- Notes:
--   - Each policy belongs to exactly one workspace (global or tenant).
--   - `name` is nullable and unique per workspace when present.
--   - `actions` is a JSONB text array (e.g. '["membership:update"]'); supports '*'.
--   - `resource` is 'self' | '<uuid>' | '*'.
--   - `constraint` is an optional DSL expression (see contracts/policy-document.md).
--   - `meta` and `tags` support lightweight extension and search.
--   - Audit is minimal: `created_at` / `updated_at` only.
CREATE TABLE IF NOT EXISTS
  policy (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
    -- Scope: policies are defined per-workspace
    workspace_id UUID NOT NULL,
    -- Policy identity
    name TEXT, -- optional human label; unique per workspace when set
    -- Policy body
    effect TEXT NOT NULL CHECK (effect IN ('allow', 'deny')),
    principal_id UUID, -- optional; defaults to the attachment target when omitted
    actions TEXT[] NOT NULL DEFAULT '{}', -- canonical action strings, e.g. 'membership:update'
    resource TEXT NOT NULL, -- 'self' | '<uuid>' | '*'
    constraint_expr TEXT, -- optional DSL expression (column avoids the reserved SQL keyword `constraint`)
    description TEXT, -- optional human-friendly description
    -- START Meta & Tags
    tags TEXT[] NOT NULL DEFAULT '{}', -- lightweight labels for grouping/search
    meta JSONB NOT NULL DEFAULT '{}'::jsonb, -- structured metadata, enforced as JSON object
    -- END Meta & Tags
    -- START Audit
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ,
    -- END Audit
    -- ---------- Constraints ----------
    CONSTRAINT policy_workspace_fk FOREIGN KEY (workspace_id) REFERENCES workspace (id) ON UPDATE CASCADE ON DELETE CASCADE,
    -- Enforce JSON object shape
    CONSTRAINT policy_meta_is_object CHECK (jsonb_typeof(meta) = 'object')
  );

-- =========================
-- Indexes
-- =========================
-- Names are unique per workspace only when present (multiple NULLs allowed).
CREATE UNIQUE INDEX IF NOT EXISTS policy_workspace_name_key ON policy (workspace_id, name)
WHERE
  name IS NOT NULL;
