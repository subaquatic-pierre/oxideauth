-- fixtures/04_credential.sql
-- Purpose: Seed initial credentials for system bootstrap accounts.
-- Notes:
--   - Links to accounts from 01_account.sql.
--   - Uses 'local' provider with placeholder password hashes.
--   - Replace password_hash values with Argon2/BCrypt hashes in real setups.
INSERT INTO
  credential (
    id,
    account_id,
    provider,
    secret,
    created_by,
    workspace_id,
    kind
  )
VALUES
  -- Root system account login
  (
    '30000000-0000-0000-0000-000000000001',
    '00000000-0000-0000-0000-000000000001', -- root account
    'local',
    '$argon2id$v=19$m=65536,t=3,p=1$ZmFrZXNhbHQ$ZmFrZXBhc3N3b3JkaGFzaA', -- placeholder hash for 'rootpass'
    '00000000-0000-0000-0000-000000000001',
    '10000000-0000-0000-0000-000000000001', -- global workspace,
    'password'
  ),
  -- Owner/admin account login
  (
    '30000000-0000-0000-0000-000000000002',
    '00000000-0000-0000-0000-000000000002', -- owner account
    'local',
    '$argon2id$v=19$m=65536,t=3,p=1$ZmFrZXNhbHQ$ZmFrZXBhc3N3b3JkaGFzaA', -- placeholder hash for 'ownerpass'
    '00000000-0000-0000-0000-000000000001',
    '10000000-0000-0000-0000-000000000001', -- global workspace,
    'password'
  ),
  -- Test account login
  (
    '30000000-0000-0000-0000-000000000003',
    '00000000-0000-0000-0000-000000000003', -- test account
    'local',
    '$argon2id$v=19$m=65536,t=3,p=1$ZmFrZXNhbHQ$ZmFrZXBhc3N3b3JkaGFzaA', -- placeholder hash for 'testpass'
    '00000000-0000-0000-0000-000000000001',
    '10000000-0000-0000-0000-000000000003', -- acme workspace,
    'password'
  ) ON CONFLICT (id)
DO NOTHING;