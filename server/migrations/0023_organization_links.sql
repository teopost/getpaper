-- Links for the people of the organization: a link with audience 'organization' can be read only by
-- members of the workspace and by signed-in people whose verified email belongs to one of the
-- organization domains of the workspace. They don't become members of the workspace.
ALTER TABLE teams ADD COLUMN organization_domains TEXT NOT NULL DEFAULT '';
ALTER TABLE shares ADD COLUMN audience TEXT NOT NULL DEFAULT 'public' CHECK (audience IN ('public', 'organization'));
