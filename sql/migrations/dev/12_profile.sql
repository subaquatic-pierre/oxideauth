-- migrations/11_profile.sql
-- Purpose: Define the workspace-scoped `profile` entity. A profile is the
--   workspace-facing identity of an account (one per account per workspace) and
--   carries display/persona data (display_name, job_title, timezone, ...) that
--   is distinct from the system-level account identity (email).
-- Notes:
--   - A profile belongs to exactly one account and one workspace.
--   - Uniqueness is enforced per (account_id, workspace_id).
--   - `meta` is a JSONB object for structured profile metadata.
--   - `created_by` / `updated_by` are audit fields; FKs can be added later if
--     bootstrap-safe (mirrors workspace/project migrations).
CREATE TABLE IF NOT EXISTS
  profile (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
    -- Identity: who and where
    account_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    -- Profile identity / presentation (workspace-facing persona)
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    description TEXT,
    display_name TEXT,
    job_title TEXT,
    timezone TEXT,
    avatar_url TEXT,
    version BIGINT NOT NULL DEFAULT 0,
    -- START Meta & Tags
    -- Lightweight labels for search/segments
    tags TEXT[] NOT NULL DEFAULT '{}',
    -- Freeform structured metadata; validated as JSON object
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
    -- ---------- Constraints ----------
    CONSTRAINT profile_account_fk FOREIGN KEY (account_id) REFERENCES account (id) ON UPDATE CASCADE ON DELETE CASCADE,
    CONSTRAINT profile_workspace_fk FOREIGN KEY (workspace_id) REFERENCES workspace (id) ON UPDATE CASCADE ON DELETE CASCADE,
    -- Enforce JSON object shape
    CONSTRAINT profile_meta_is_object CHECK (jsonb_typeof(meta) = 'object'),
    CONSTRAINT profile_audit_is_object CHECK (jsonb_typeof(audit) = 'object')
  );

-- =========================
-- Indexes
-- =========================
-- One profile per account per workspace
CREATE UNIQUE INDEX IF NOT EXISTS profile_account_workspace_key ON profile (account_id, workspace_id);

-- Fast lookup of all profiles within a workspace
CREATE INDEX IF NOT EXISTS profile_by_workspace ON profile (workspace_id);

CREATE UNIQUE INDEX IF NOT EXISTS profile_workspace_email_lower_key ON profile (workspace_id, lower(email));