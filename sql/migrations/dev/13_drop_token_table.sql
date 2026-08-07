-- migration 13: drop the token blacklist table
-- Purpose: Remove legacy blacklist infrastructure. Token revocation is now
-- handled via version-based invalidation (see account.token_version and
-- membership.token_version columns).

DROP TABLE IF EXISTS token CASCADE;
