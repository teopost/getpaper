-- What an API key (REST API and MCP server) may do. The effective permission is the lower of the
-- key's and the user's: read keys act as viewers, write keys at most as editors.
ALTER TABLE api_keys ADD COLUMN access TEXT NOT NULL DEFAULT 'write' CHECK (access IN ('read', 'write'));
ALTER TABLE api_keys ADD COLUMN expires_at TIMESTAMPTZ;
-- every workspace the user belongs to (also future ones), or only those in api_key_workspaces;
-- api_keys.team_id is the default workspace
ALTER TABLE api_keys ADD COLUMN all_workspaces BOOLEAN NOT NULL DEFAULT false;
-- when true, only the collections in api_key_collections
ALTER TABLE api_keys ADD COLUMN collections_restricted BOOLEAN NOT NULL DEFAULT false;

CREATE TABLE api_key_workspaces (
    api_key_id UUID NOT NULL REFERENCES api_keys(id) ON DELETE CASCADE,
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    PRIMARY KEY (api_key_id, team_id)
);
INSERT INTO api_key_workspaces (api_key_id, team_id) SELECT id, team_id FROM api_keys;

CREATE TABLE api_key_collections (
    api_key_id UUID NOT NULL REFERENCES api_keys(id) ON DELETE CASCADE,
    collection_id UUID NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    PRIMARY KEY (api_key_id, collection_id)
);
