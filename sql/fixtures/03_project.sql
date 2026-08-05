-- fixtures/03_project.sql
-- Purpose: Seed initial projects under workspaces.
-- Notes:
--   - Projects are children of workspaces.
--   - Linked to the workspaces created in 02_workspace.sql.
--   - Created_by must reference a valid account (01_account.sql).
INSERT INTO
  project (id, workspace_id, name, description, created_by)
VALUES
  -- System bootstrap project inside the global workspace
  (
    '20000000-0000-0000-0000-000000000001',
    '10000000-0000-0000-0000-000000000001', -- global workspace
    'system',
    'System bootstrap project (reserved)',
    '00000000-0000-0000-0000-000000000001'
  ),
  -- Example Acme project inside the Acme workspace
  (
    '20000000-0000-0000-0000-000000000002',
    '10000000-0000-0000-0000-000000000002', -- acme workspace
    'acme-demo',
    'Demo project for Acme Corp',
    '00000000-0000-0000-0000-000000000002'
  ) ON CONFLICT (id)
DO NOTHING;