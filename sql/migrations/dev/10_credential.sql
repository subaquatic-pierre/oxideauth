-- migrations/10_credential.sql
-- Purpose: Store authentication credentials tied to accounts within a workspace.
-- Notes:
--   - A credential represents one login method (password, OAuth, SSO, API key).
--   - A credential is anchored to exactly one membership (`membership_id NOT NULL`);
--     the composite FK `cred_membership_fk` on (membership_id, account_id, workspace_id)
--     guarantees account/workspace consistency between credential and membership.
--   - Scoped by `workspace_id` to allow multi-tenant isolation.
--   - Per-kind cardinality:
--       * password: 1 active per account per workspace (cred_unique_active_password)
--       * oauth/sso: 1 active per provider per account per workspace
--         (cred_unique_active_provider_per_account); also 1 active per
--         provider + provider_id per workspace (cred_unique_active_provider)
--       * api_key: unbounded
--   - `created_by` / `updated_by` are audit fields
CREATE TABLE IF NOT EXISTS
  credential (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
    -- Who (account identity this credential belongs to)
    account_id UUID NOT NULL,
    -- Where (workspace/tenant scope for the credential)
    workspace_id UUID NOT NULL,
    -- Membership anchor: the credential authenticates as exactly one membership
    membership_id UUID NOT NULL,
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
    -- Anchor to exactly one membership; composite key keeps account/workspace consistent
    CONSTRAINT cred_membership_fk FOREIGN KEY (membership_id, account_id, workspace_id) REFERENCES membership (id, account_id, workspace_id) ON UPDATE CASCADE ON DELETE CASCADE,
    -- Restrict to known credential kinds
    CONSTRAINT cred_kind_chk CHECK (kind IN ('password', 'oauth', 'sso', 'api_key')),
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

-- Per-account provider cardinality:
-- Enforce uniqueness: one active OAuth/SSO credential per provider per account per workspace.
CREATE UNIQUE INDEX IF NOT EXISTS cred_unique_active_provider_per_account ON credential (workspace_id, account_id, provider)
WHERE
  kind IN ('oauth', 'sso')
  AND status = 'active';

-- Membership-scoped credential listing/invalidation
CREATE INDEX IF NOT EXISTS credential_membership_id_idx ON credential (membership_id);
