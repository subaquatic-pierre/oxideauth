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
    CONSTRAINT cred_audit_is_object CHECK (jsonb_typeof(audit) = 'object')
    -- (meta also constrained if needed in a follow-up migration)
  );

-- TODO: Make sure to re-enable workspace-user-credential unique constraints
-- =========================
-- Indexes
-- =========================
-- Password lookup:
-- Enforce uniqueness of one active password credential per workspace/email.
-- CREATE INDEX IF NOT EXISTS cred_password_lookup_idx ON credential (workspace_id, lower(email))
-- WHERE
--   kind = 'password'
--   AND status = 'active'
--   AND email IS NOT NULL;
-- CREATE UNIQUE INDEX cred_pw_unique_ns_email ON credential (workspace_id, lower(email))
-- WHERE
--   kind = 'password'
--   AND status = 'active'
--   AND email IS NOT NULL;
-- OAuth/SSO lookup:
-- Uniqueness enforced by provider + provider_id per workspace.
-- CREATE UNIQUE INDEX IF NOT EXISTS cred_oauth_lookup_idx ON credential (workspace_id, provider, provider_id)
-- WHERE
--   kind IN ('oauth', 'sso')
--   AND status = 'active'
--   AND provider_id IS NOT NULL;
-- CREATE UNIQUE INDEX cred_oauth_unique_ns ON credential (workspace_id, provider, provider_id)
-- WHERE
--   kind IN ('oauth', 'sso')
--   AND provider_id IS NOT NULL;