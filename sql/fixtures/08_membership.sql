-- fixtures/08_membership.sql
-- Purpose: Seed memberships linking accounts to workspaces and projects.
-- Notes:
--   - Membership ties an account to a workspace or project.
--   - Scope is either 'workspace' or 'project'.
--   - Status defaults to 'active'.
--   - Roles will be attached later via membership_role (09).
INSERT INTO
  membership (
    id,
    account_id,
    workspace_id,
    scope,
    project_id,
    status,
    created_by
  )
VALUES
  -- Root account as sysadmin in global workspace
  (
    '60000000-0000-0000-0000-000000000001',
    '00000000-0000-0000-0000-000000000001', -- root account
    '10000000-0000-0000-0000-000000000001', -- global workspace
    'workspace',
    NULL,
    'active',
    '00000000-0000-0000-0000-000000000001'
  ),
  -- Owner account in global workspace
  (
    '60000000-0000-0000-0000-000000000002',
    '00000000-0000-0000-0000-000000000002', -- owner account
    '10000000-0000-0000-0000-000000000001', -- global workspace
    'workspace',
    NULL,
    'active',
    '00000000-0000-0000-0000-000000000001'
  ),
  -- Owner account in acme workspace
  (
    '60000000-0000-0000-0000-000000000003',
    '00000000-0000-0000-0000-000000000002', -- owner account
    '10000000-0000-0000-0000-000000000002', -- acme workspace
    'workspace',
    NULL,
    'active',
    '00000000-0000-0000-0000-000000000001'
  ),
  -- Test account in acme-demo project
  (
    '60000000-0000-0000-0000-000000000004',
    '00000000-0000-0000-0000-000000000003', -- test account
    '10000000-0000-0000-0000-000000000002', -- acme workspace
    'project',
    '20000000-0000-0000-0000-000000000002', -- acme-demo project
    'active',
    '00000000-0000-0000-0000-000000000002'
  );