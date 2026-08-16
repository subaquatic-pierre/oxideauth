ALTER TABLE profile ADD COLUMN IF NOT EXISTS email TEXT;

UPDATE profile p
SET email = a.email
FROM account a
WHERE p.account_id = a.id
  AND p.email IS NULL;

ALTER TABLE profile ALTER COLUMN email SET NOT NULL;
