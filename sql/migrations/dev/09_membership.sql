-- migrations/09_membership.sql
-- Purpose: Define memberships linking accounts to workspaces or projects.
-- Notes:
--   - Each membership ties an account to a workspace, with optional project-level scope.
--   - `scope` distinguishes workspace vs. project membership.
--   - `status` reflects lifecycle (invited, active, suspended).
--   - `profile_id` is the required workspace-facing identity of the linked account.
--   - Partial unique indexes enforce one membership per account per workspace/project.
--   - Active memberships are indexed for fast auth lookups.
--   - `membership_id_ns_unique` (id, account_id, workspace_id) is the FK target for
--     credential's composite link (cred_membership_fk in 10_credential.sql).
--   - `created_by` / `updated_by` are audit fields; FKs can be added later if bootstrap-safe.
CREATE TABLE IF NOT EXISTS
  membership (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
    -- Identity: who and where
    account_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    version BIGINT NOT NULL DEFAULT 0,
    scope TEXT NOT NULL, -- 'workspace' | 'project'
    project_id UUID, -- set only when scope='project'
    profile_id UUID NOT NULL, -- workspace-facing identity of the linked account
    -- Lifecycle state
    status TEXT NOT NULL DEFAULT 'active', -- 'invited'|'active'|'suspended'
    -- START Meta & Tags
    tags TEXT[] NOT NULL DEFAULT '{}', -- lightweight labels
    meta JSONB NOT NULL DEFAULT '{}'::jsonb, -- structured metadata
    -- END Meta & Tags
    -- START Audit
    created_by UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by UUID,
    updated_at TIMESTAMPTZ,
    audit JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- END Audit
    -- ---------- Constraints ----------
    CONSTRAINT mem_account_fk FOREIGN KEY (account_id) REFERENCES account (id) ON UPDATE CASCADE ON DELETE CASCADE,
    CONSTRAINT mem_workspace_fk FOREIGN KEY (workspace_id) REFERENCES workspace (id) ON UPDATE CASCADE ON DELETE CASCADE,
    CONSTRAINT mem_project_fk FOREIGN KEY (project_id) REFERENCES project (id) ON UPDATE CASCADE ON DELETE CASCADE,
    CONSTRAINT membership_profile_fk FOREIGN KEY (profile_id) REFERENCES profile (id) ON UPDATE CASCADE ON DELETE RESTRICT,
    CONSTRAINT membership_profile_owner_fk FOREIGN KEY (profile_id, account_id, workspace_id)
      REFERENCES profile (id, account_id, workspace_id) ON UPDATE CASCADE ON DELETE RESTRICT,
    -- Composite FK target for credential (guarantees account/workspace consistency)
    CONSTRAINT membership_id_ns_unique UNIQUE (id, account_id, workspace_id),
    -- Enforce JSON object shape
    CONSTRAINT membership_meta_is_object CHECK (jsonb_typeof(meta) = 'object'),
    CONSTRAINT membership_audit_is_object CHECK (jsonb_typeof(audit) = 'object'),
    -- Shape guard: project_id required only for project-scoped memberships
    CONSTRAINT membership_scope_shape_chk CHECK (
      (
        scope = 'workspace'
        AND project_id IS NULL
      )
      OR (
        scope = 'project'
        AND project_id IS NOT NULL
      )
    )
  );

-- =========================
-- Indexes
-- =========================
-- Intent-expressing partial uniques: only one membership per account per workspace/project
CREATE UNIQUE INDEX IF NOT EXISTS membership_ns_unique ON membership (account_id, workspace_id)
WHERE
  scope = 'workspace';

CREATE UNIQUE INDEX IF NOT EXISTS membership_proj_unique ON membership (account_id, project_id)
WHERE
  scope = 'project';

-- Lookups used in auth for fast active membership checks
CREATE INDEX IF NOT EXISTS membership_by_ns ON membership (workspace_id, account_id)
WHERE
  scope = 'workspace'
  AND status = 'active';

CREATE INDEX IF NOT EXISTS membership_by_proj ON membership (project_id, account_id)
WHERE
  scope = 'project'
  AND status = 'active';

-- Lookup by linked profile (workspace-facing identity)
CREATE INDEX IF NOT EXISTS membership_by_profile ON membership (profile_id)
;
