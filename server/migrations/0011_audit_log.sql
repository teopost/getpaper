-- Audit log (Settings → Activity, General settings → Audit log): sign-ins and the actions of people.
-- Kept for the number of days set in the general settings (0 = nothing is recorded).
CREATE TABLE events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- the workspace; NULL for events of the installation (sign-ins, general settings, workspaces)
    team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
    -- who did it; name and email stay readable after the account is deleted
    actor_id UUID REFERENCES users(id) ON DELETE SET NULL,
    actor_name TEXT,
    actor_email TEXT,
    -- e.g. auth.signin, documents.publish, users.role
    name TEXT NOT NULL,
    -- what it is about: document, collection, user, group, share, api_key, workspace
    target_type TEXT,
    target_id UUID,
    target_name TEXT,
    data JSONB NOT NULL DEFAULT '{}',
    ip TEXT,
    user_agent TEXT,
    -- web, api (API key) or mcp
    via TEXT
);
CREATE INDEX events_team_idx ON events (team_id, created_at DESC);
CREATE INDEX events_actor_idx ON events (actor_id, created_at DESC);
CREATE INDEX events_created_idx ON events (created_at);
