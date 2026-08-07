-- migration 12: add token_version to membership table
-- Purpose: Version-based token revocation — incrementing this version
-- invalidates all tokens issued for this membership before the increment.

ALTER TABLE membership
  ADD COLUMN IF NOT EXISTS token_version BIGINT NOT NULL DEFAULT 0;
