-- fixtures/06_permission.sql
-- Purpose: Seed initial permissions for the system.
-- Notes:
--   - Permissions are the atomic units of authorization.
--   - They are human-readable (name is PK).
--   - Will be linked to roles in fixtures/07_role_permission.sql.
INSERT INTO
  permission (name, id, description, created_by, workspace_id)
VALUES
  -- Account management
  (
    'account:create',
    '50000000-0000-0000-0000-000000000001',
    'Create new accounts',
    '00000000-0000-0000-0000-000000000001',
    '10000000-0000-0000-0000-000000000001'
  ),
  (
    'account:update',
    '50000000-0000-0000-0000-000000000002',
    'Update existing accounts',
    '00000000-0000-0000-0000-000000000001',
    '10000000-0000-0000-0000-000000000001'
  ),
  (
    'account:delete',
    '50000000-0000-0000-0000-000000000003',
    'Delete accounts',
    '00000000-0000-0000-0000-000000000001',
    '10000000-0000-0000-0000-000000000001'
  ),
  -- Workspace/project management
  (
    'workspace:create',
    '50000000-0000-0000-0000-000000000004',
    'Create workspaces',
    '00000000-0000-0000-0000-000000000001',
    '10000000-0000-0000-0000-000000000002'
  ),
  (
    'project:create',
    '50000000-0000-0000-0000-000000000005',
    'Create projects under a workspace',
    '00000000-0000-0000-0000-000000000001',
    '10000000-0000-0000-0000-000000000002'
  ),
  -- Memberships
  (
    'membership:invite',
    '50000000-0000-0000-0000-000000000006',
    'Invite accounts into a workspace/project',
    '00000000-0000-0000-0000-000000000001',
    '10000000-0000-0000-0000-000000000003'
  ),
  (
    'membership:suspend',
  '50000000-0000-0000-0000-000000000007',
  'Suspend memberships',
  '00000000-0000-0000-0000-000000000001',
  '10000000-0000-0000-0000-000000000003'
  )
ON CONFLICT (name) DO NOTHING;