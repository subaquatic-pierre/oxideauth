-- fixtures/05_role.sql
-- Purpose: Seed initial roles for workspaces and projects.
-- Notes:
--   - Roles are reusable permission bundles.
--   - Linked later to memberships via membership_role.
--   - Keep role names unique and human-readable.
INSERT INTO
  role (id, name, description, created_by, workspace_id)
VALUES
  -- Global/system roles
  (
    '40000000-0000-0000-0000-000000000001',
    'sysadmin',
    'System administrator with full privileges',
    '00000000-0000-0000-0000-000000000001',
    '10000000-0000-0000-0000-000000000001'
  ),
  (
    '40000000-0000-0000-0000-000000000002',
    'owner',
    'Workspace/project owner with management privileges',
    '00000000-0000-0000-0000-000000000001',
    '10000000-0000-0000-0000-000000000001'
  ),
  (
    '40000000-0000-0000-0000-000000000003',
    'member',
    'Standard member with limited access',
    '00000000-0000-0000-0000-000000000001',
    '10000000-0000-0000-0000-000000000002'
  ),
  (
    '40000000-0000-0000-0000-000000000004',
    'viewer',
    'Read-only viewer role',
    '00000000-0000-0000-0000-000000000001',
    '10000000-0000-0000-0000-000000000003'
  ) ON CONFLICT (id)
DO NOTHING;