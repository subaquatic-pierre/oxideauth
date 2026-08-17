-- migrations/13_membership_policy.sql
-- Purpose: Define many-to-many relationships between memberships and policies.
-- Notes:
--   - A policy attached to a membership applies to that single membership only.
--   - Primary key is composite (membership_id, policy_id) to prevent duplicates.
--   - Cascading: deleting a membership or a policy cascades its bindings.
CREATE TABLE IF NOT EXISTS
  membership_policy (
    membership_id UUID NOT NULL,
    policy_id UUID NOT NULL,
    workspace_id UUID,
    -- Composite PK ensures uniqueness
    PRIMARY KEY (membership_id, policy_id),
    -- FKs
    CONSTRAINT mp_membership_fk FOREIGN KEY (membership_id) REFERENCES membership (id) ON UPDATE CASCADE ON DELETE CASCADE,
    CONSTRAINT mp_policy_fk FOREIGN KEY (policy_id) REFERENCES policy (id) ON UPDATE CASCADE ON DELETE CASCADE
  );

-- =========================
-- Indexes
-- =========================
-- Speed up joins and lookups by membership or policy
CREATE INDEX IF NOT EXISTS mp_membership_idx ON membership_policy (membership_id);

CREATE INDEX IF NOT EXISTS mp_policy_idx ON membership_policy (policy_id);
