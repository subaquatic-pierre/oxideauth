-- migrations/04_credential.sql
-- Purpose: Store authentication credentials tied to accounts within a workspace.
-- Notes:
--   - A credential represents one login method (password, OAuth, SSO, API key).
--   - Scoped by `workspace_id` to allow multi-tenant isolation.
--   - Only one active password credential per (workspace, email).
--   - OAuth/SSO credentials use `provider` + `provider_id` for uniqueness.
--   - `created_by` / `updated_by` are audit fields
CREATE TABLE IF NOT EXISTS
  credential (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
    -- Who (account identity this credential belongs to)
    account_id UUID NOT NULL,
    -- Where (workspace/tenant scope for the credential)
    workspace_id UUID NOT NULL,
    -- How (authentication kind: password, oauth, sso, api_key)
    kind TEXT NOT NULL, -- 'password','oauth','sso','api_key'
    -- External identity provider metadata
    provider TEXT NOT NULL, -- e.g. 'local','google','github','saml'
    provider_id TEXT, -- external subject/user id (for oauth/sso)
    -- Login details
    email TEXT, -- email for password login or IdP email
    secret TEXT, -- only populated if kind='password'
    -- Credential state (active, revoked, pending)
    status TEXT NOT NULL DEFAULT 'active', -- 'active','revoked','pending'
    -- Last time this credential was used for authentication
    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
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
    CONSTRAINT cred_account_fk FOREIGN KEY (account_id) REFERENCES account (id) ON UPDATE CASCADE ON DELETE CASCADE,
    CONSTRAINT cred_workspace_fk FOREIGN KEY (workspace_id) REFERENCES workspace (id) ON UPDATE CASCADE ON DELETE CASCADE,
    -- Ensure audit JSON is always an object
    CONSTRAINT cred_audit_is_object CHECK (jsonb_typeof(audit) = 'object'),
    CONSTRAINT cred_config_is_object CHECK (jsonb_typeof(config) = 'object')
    -- (meta also constrained if needed in a follow-up migration)
  );

-- =========================
-- Indexes
-- =========================
-- Password lookup:
-- Enforce uniqueness: one active password credential per workspace + account.
CREATE UNIQUE INDEX IF NOT EXISTS cred_unique_active_password ON credential (workspace_id, account_id)
WHERE
  kind = 'password'
  AND status = 'active';

-- OAuth/SSO lookup:
-- Enforce uniqueness: one active OAuth/SSO credential per workspace + provider + provider_id.
CREATE UNIQUE INDEX IF NOT EXISTS cred_unique_active_provider ON credential (workspace_id, provider, provider_id)
WHERE
  kind IN ('oauth', 'sso')
  AND status = 'active'
  AND provider_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS credential_membership_id_idx ON credential (membership_id);