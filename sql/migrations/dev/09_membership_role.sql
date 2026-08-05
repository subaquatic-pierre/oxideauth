-- migrations/09_membership_role.sql
-- Purpose: Bridge table linking memberships to roles (many-to-many).
-- Notes:
--   - Each membership can be granted multiple roles.
--   - Composite PK enforces uniqueness (no duplicate role per membership).
--   - Cascade on membership deletion ensures cleanup; role deletion is restricted.
--   - Indexes improve lookup by membership or role.
-- =========================
-- UP
-- =========================
CREATE TABLE IF NOT EXISTS
  membership_role (
    membership_id UUID NOT NULL,
    role_id UUID NOT NULL,
    -- Composite PK ensures uniqueness
    PRIMARY KEY (membership_id, role_id),
    -- FKs
    CONSTRAINT mr_membership_fk FOREIGN KEY (membership_id) REFERENCES membership (id) ON UPDATE CASCADE ON DELETE CASCADE,
    CONSTRAINT mr_role_fk FOREIGN KEY (role_id) REFERENCES role (id) ON UPDATE CASCADE ON DELETE RESTRICT
  );

-- Indexes to accelerate joins
CREATE INDEX IF NOT EXISTS mr_membership_idx ON membership_role (membership_id);

CREATE INDEX IF NOT EXISTS mr_role_idx ON membership_role (role_id);