-- fixtures/02_workspace.sql
-- Purpose: Seed initial workspaces for system bootstrap.
-- Notes:
--   - Workspaces represent tenants/domains.
--   - The global workspace (id=...001) acts as the system root.
--   - Owned by root or owner accounts from fixtures/01_account.sql.
INSERT INTO
  workspace (id, name, slug, description, created_by)
VALUES
  -- Global/system workspace
  (
    '10000000-0000-0000-0000-000000000001',
    'global',
    'global',
    'System-wide global workspace',
    '00000000-0000-0000-0000-000000000001'
  ),
  -- Example tenant workspace (owned by owner account)
  (
    '10000000-0000-0000-0000-000000000002',
    'registrar',
    'registrar',
    'Registrar Domain To List all Workspaces',
    '00000000-0000-0000-0000-000000000002'
  ),
  (
    '10000000-0000-0000-0000-000000000003',
    'acme',
    'acme',
    'Example tenant workspace for Acme Corp',
    '00000000-0000-0000-0000-000000000002'
  ) ON CONFLICT (id)
DO NOTHING;