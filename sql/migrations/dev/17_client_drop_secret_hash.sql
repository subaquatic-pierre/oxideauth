-- migrations/17_client_drop_secret_hash.sql
-- Purpose: Remove the client-secret mechanism in favor of credential-based
--   (API-key) client authentication. The `secret_hash` column is no longer
--   written or read by the application (CredentialService::authenticate is the
--   new client-auth path); drop the column and its data.

ALTER TABLE client DROP COLUMN IF EXISTS secret_hash;
