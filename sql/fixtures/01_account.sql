-- fixtures/01_account.sql
-- Purpose: Seed initial system accounts for development, testing, and bootstrap.
-- Notes:
--   - These accounts are referenced by fixtures in later files (e.g., workspaces, roles).
--   - Passwords should be handled securely in production; for fixtures we use placeholders.
INSERT INTO
  account (id, email, name, verified, enabled, created_by)
VALUES
  -- Root system account
  (
    '00000000-0000-0000-0000-000000000001',
    'root@system.local',
    'Root Account',
    TRUE,
    TRUE,
    '00000000-0000-0000-0000-000000000001'
  ),
  -- Owner/admin account
  (
    '00000000-0000-0000-0000-000000000002',
    'owner@system.local',
    'Owner Account',
    TRUE,
    TRUE,
    '00000000-0000-0000-0000-000000000001'
  ),
  -- Example test account
  (
    '00000000-0000-0000-0000-000000000003',
    'test@example.com',
    'Test Account',
    TRUE,
    TRUE,
    '00000000-0000-0000-0000-000000000001'
  ) ON CONFLICT (id)
DO NOTHING;