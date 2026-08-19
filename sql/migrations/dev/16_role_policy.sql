-- migrations/12_role_policy.sql
-- Purpose: Define many-to-many relationships between roles and policies.
-- Notes:
--   - A policy attached to a role applies to every membership holding the role.
--   - Primary key is composite (role_id, policy_id) to prevent duplicates.
--   - Cascading: deleting a role or a policy cascades its bindings.
CREATE TABLE IF NOT EXISTS
  role_policy (
    role_id UUID NOT NULL,
    policy_id UUID NOT NULL,
    workspace_id UUID,
    -- Composite PK ensures uniqueness
    PRIMARY KEY (role_id, policy_id),
    -- FKs
    CONSTRAINT rp_role_fk FOREIGN KEY (role_id) REFERENCES role (id) ON UPDATE CASCADE ON DELETE CASCADE,
    CONSTRAINT rp_policy_fk FOREIGN KEY (policy_id) REFERENCES policy (id) ON UPDATE CASCADE ON DELETE CASCADE
  );

-- =========================
-- Indexes
-- =========================
-- Speed up joins and lookups by role or policy
CREATE INDEX IF NOT EXISTS rp_role_idx ON role_policy (role_id);

CREATE INDEX IF NOT EXISTS rp_policy_idx ON role_policy (policy_id);
