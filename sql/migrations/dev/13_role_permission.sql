-- migrations/07_role_permission.sql
-- Purpose: Define many-to-many relationships between roles and permissions.
-- Notes:
--   - Each role can have multiple permissions; each permission can belong to multiple roles.
--   - Primary key is composite (role_id, permission_id) to prevent duplicates.
--   - Cascading: deleting a role cascades its bindings; deleting a permission is restricted.
--   - Workspace consistency (role.workspace_id = permission.workspace_id) should be enforced
--     via a trigger or deferred check (see TODOs).
CREATE TABLE IF NOT EXISTS
  role_permission (
    -- Composite key ensures uniqueness
    role_id UUID NOT NULL,
    permission_id UUID NOT NULL,
    workspace_id UUID,
    PRIMARY KEY (role_id, permission_id),
    -- FKs
    CONSTRAINT rp_role_fk FOREIGN KEY (role_id) REFERENCES role (id) ON UPDATE CASCADE ON DELETE CASCADE,
    CONSTRAINT rp_permission_fk FOREIGN KEY (permission_id) REFERENCES permission (id) ON UPDATE CASCADE ON DELETE RESTRICT
  );

-- =========================
-- Indexes
-- =========================
-- Speed up joins and lookups by role or permission
CREATE INDEX IF NOT EXISTS rp_role_idx ON role_permission (role_id);

CREATE INDEX IF NOT EXISTS rp_permission_idx ON role_permission (permission_id);