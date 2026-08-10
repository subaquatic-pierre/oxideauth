-- fixtures/07_role_permission.sql
-- Purpose: Map roles to permissions.
-- Notes:
--   - A role can have many permissions.
--   - Permissions are attached using the role_permission join table.
--   - Ensure all role_id and permission names exist from previous fixtures.
INSERT INTO
  role_permission (role_id, permission_id)
VALUES
  -- sysadmin: full access
  (
    '40000000-0000-0000-0000-000000000001',
    '50000000-0000-0000-0000-000000000001'
  ),
  (
    '40000000-0000-0000-0000-000000000001',
    '50000000-0000-0000-0000-000000000002'
  ),
  (
    '40000000-0000-0000-0000-000000000001',
    '50000000-0000-0000-0000-000000000003'
  ),
  (
    '40000000-0000-0000-0000-000000000001',
    '50000000-0000-0000-0000-000000000004'
  ),
  (
    '40000000-0000-0000-0000-000000000001',
    '50000000-0000-0000-0000-000000000005'
  ),
  (
    '40000000-0000-0000-0000-000000000001',
    '50000000-0000-0000-0000-000000000006'
  ),
  (
    '40000000-0000-0000-0000-000000000001',
    '50000000-0000-0000-0000-000000000007'
  ),
  -- owner: manage projects + memberships
  (
    '40000000-0000-0000-0000-000000000002',
    '50000000-0000-0000-0000-000000000005'
  ),
  (
    '40000000-0000-0000-0000-000000000002',
    '50000000-0000-0000-0000-000000000006'
  ),
  (
    '40000000-0000-0000-0000-000000000002',
    '50000000-0000-0000-0000-000000000007'
  )
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- member: no special permissions (can inherit defaults via app logic)
-- viewer: no special permissions (read-only enforced via RLS/policy)