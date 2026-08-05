-- fixtures/09_membership_role.sql
-- Purpose: Attach roles to memberships.
-- Notes:
--   - Each membership can have multiple roles.
--   - Links via the membership_role join table.
--   - Role IDs come from fixtures/05_role.sql.
--   - Membership IDs come from fixtures/08_membership.sql.
INSERT INTO
  membership_role (membership_id, role_id)
VALUES
  -- Root membership in global workspace → sysadmin
  (
    '60000000-0000-0000-0000-000000000001', -- root/global
    '40000000-0000-0000-0000-000000000001'
  ), -- sysadmin
  -- Owner membership in global workspace → owner
  (
    '60000000-0000-0000-0000-000000000002', -- owner/global
    '40000000-0000-0000-0000-000000000002'
  ), -- owner
  -- Owner membership in acme workspace → owner
  (
    '60000000-0000-0000-0000-000000000003', -- owner/acme
    '40000000-0000-0000-0000-000000000002'
  ), -- owner
  -- Test account in acme-demo project → member
  (
    '60000000-0000-0000-0000-000000000004', -- test/acme-demo
    '40000000-0000-0000-0000-000000000003'
  ) -- member
  ON CONFLICT
DO NOTHING;