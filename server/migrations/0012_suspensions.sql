-- Suspended accounts (superadmins, the whole installation) and suspended members (workspace
-- admins, one workspace). Suspended people can't sign in or open the workspace; their content,
-- role, groups and permissions are kept for when they are reactivated.
ALTER TABLE users ADD COLUMN suspended_at TIMESTAMPTZ;
ALTER TABLE users ADD COLUMN suspended_by UUID REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE team_memberships ADD COLUMN suspended_at TIMESTAMPTZ;
ALTER TABLE team_memberships ADD COLUMN suspended_by UUID REFERENCES users(id) ON DELETE SET NULL;
