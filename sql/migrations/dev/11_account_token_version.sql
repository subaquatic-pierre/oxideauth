-- migration 11: add token_version to account table
-- Purpose: Version-based token revocation — incrementing this version
-- invalidates all tokens issued before the increment.

ALTER TABLE account
  ADD COLUMN IF NOT EXISTS token_version BIGINT NOT NULL DEFAULT 0;
