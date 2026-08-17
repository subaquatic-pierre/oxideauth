-- migrations/12_membership_profile_id.sql
-- Purpose: Backfill the `profile_id` linkage on the `membership` table.
--   The membership entity/model surface `profile_id` (workspace-facing identity
--   of the linked account) but the original membership migration (08) predates
--   the profile entity (11) and never received the column. This closes that gap.
-- Notes:
--   - `profile_id` is nullable: memberships created via the legacy `account_id`
--     path have no profile until one is resolved.
--   - ON DELETE SET NULL keeps a membership alive if its profile is removed;
--     the profile FK also cascades from account/workspace deletion.
ALTER TABLE membership
  ADD COLUMN IF NOT EXISTS profile_id UUID;

ALTER TABLE membership
  ADD CONSTRAINT membership_profile_fk
  FOREIGN KEY (profile_id) REFERENCES profile (id) ON UPDATE CASCADE ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS membership_by_profile ON membership (profile_id)
WHERE
  profile_id IS NOT NULL;
