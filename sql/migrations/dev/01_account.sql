-- migrations/01_account.sql
-- Purpose: Define global user identities (accounts) with verification and a global enable/disable switch.
-- Notes:
--   - `verified` indicates identity confirmation (e.g., email link, SMS, IdP proof).
--   - `enabled` is a global kill-switch to disable all access for this account.
--   - `created_by` / `updated_by` are audit fields; FK constraints can be added in a later migration
--     to avoid bootstrap problems (see TODOs).
CREATE TABLE IF NOT EXISTS
  account (
    -- Primary identity key for users (globally unique across all workspaces/projects)
    id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
    -- Canonical login/email address (enforce uniqueness via the partial index below).
    -- Keep raw-cased input; queries should use lower(email) to ensure case-insensitivity.
    email TEXT NOT NULL,
    -- Display name (profile). Kept separate from email.
    name TEXT NOT NULL,
    -- Optional profile fields
    avatar_url TEXT,
    description TEXT,
    version BIGINT NOT NULL DEFAULT 0,
    kind TEXT NOT NULL, -- 'user', 'service',
    -- Global identity state flags:
    --   verified = identity confirmed (email/SMS/IdP), blocks login pre-tenant if false
    --   enabled  = global access switch; if false, user cannot authenticate anywhere
    verified BOOLEAN NOT NULL DEFAULT FALSE,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    -- START Meta & Tags
    -- Lightweight labeling for search/segments; TEXT[] is simple and indexable
    tags TEXT[] NOT NULL DEFAULT '{}',
    -- Freeform structured metadata; keep as JSONB object (validate with CHECK below)
    meta JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- END Meta & Tags
    -- START Audit
    -- Who created this row and when. `created_by` is NOT NULL to maintain a full audit trail.
    -- Consider a bootstrap strategy (see TODOs) for the very first account.
    created_by UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Who last updated this row and when. `updated_by` may be NULL if never updated.
    updated_by UUID,
    updated_at TIMESTAMPTZ,
    -- Flexible audit payload for origin/IP/UA/etc; enforced to be a JSON object.
    audit JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- END Audit
    -- Constraints: ensure JSONB columns are objects (avoid arrays/scalars in these fields)
    CONSTRAINT account_audit_is_object CHECK (jsonb_typeof(audit) = 'object'),
    CONSTRAINT account_meta_is_object CHECK (jsonb_typeof(meta) = 'object')
    -- (Foreign keys for created_by/updated_by can be added in a follow-up migration to avoid
    --  circular bootstrapping; see TODOs.)
  );

-- Optional global uniqueness (case-insensitive) on email.
-- The WHERE clause keeps this as a partial index, allowing future NULL emails if needed.
-- If you never allow NULL emails, you can drop the WHERE to make it a plain unique index.
CREATE UNIQUE INDEX IF NOT EXISTS account_email_lower_key ON account (lower(email))
WHERE
  email IS NOT NULL;