-- migrations/15_profile_email_unique.sql
-- Purpose: Enforce workspace-scoped, case-insensitive uniqueness of profile.email.
--   (1) Reconcile any pre-existing duplicates: keep the earliest profile per
--       (workspace_id, lower(email)); reassign the email of the remaining rows
--       to a unique derived value (no rows are deleted — profiles may still be
--       referenced by memberships).
--   (2) Add a unique index on (workspace_id, lower(email)).

WITH ranked AS (
    SELECT
        id,
        row_number() OVER (
            PARTITION BY workspace_id, lower(email)
            ORDER BY created_at ASC, id ASC
        ) AS rn
    FROM profile
)
UPDATE profile p
SET email = p.email || '+' || p.id::text
FROM ranked r
WHERE p.id = r.id
  AND r.rn > 1;

CREATE UNIQUE INDEX IF NOT EXISTS profile_workspace_email_lower_key
    ON profile (workspace_id, lower(email));
