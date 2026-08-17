-- migrations/16_credential_client_auth.sql
-- Purpose: Enable credential-based client authentication (API-key style).
--   (1) membership_id links a credential to the membership whose roles/permissions
--       authorize the client. Nullable: existing user-login credentials (password,
--       oauth, sso) are linked to an account, not a membership; only api_key
--       credentials carry a membership link (enforced at the application layer).
--   (2) expires_at sets a credential expiry; NULL means non-expiring.
--   The `status` column is already TEXT with no CHECK constraint, so the new
--   'disabled' lifecycle state needs no schema change here.

ALTER TABLE credential
    ADD COLUMN IF NOT EXISTS membership_id UUID,
    ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ;

-- Link a client credential to its authorizing membership; cascade on membership
-- removal (a credential whose membership no longer exists must not authenticate).
ALTER TABLE credential
    DROP CONSTRAINT IF EXISTS cred_membership_fk;

ALTER TABLE credential
    ADD CONSTRAINT cred_membership_fk
        FOREIGN KEY (membership_id) REFERENCES membership (id)
        ON UPDATE CASCADE ON DELETE CASCADE;

-- Fast lookup for client authentication by credential id (already the PK); add an
-- index on membership_id for membership-scoped credential listing/invalidation.
CREATE INDEX IF NOT EXISTS credential_membership_id_idx
    ON credential (membership_id);
