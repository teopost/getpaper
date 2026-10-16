-- Groups of people in a workspace (as in Outline): they can be given access to collections and
-- mentioned in documents (@group notifies every member)
CREATE TABLE groups (
    id UUID PRIMARY KEY,
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    -- the group can't be mentioned
    disable_mentions BOOLEAN NOT NULL DEFAULT false,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX groups_team_name_idx ON groups (team_id, lower(name));

-- "admin" members can rename the group and manage its members, like workspace admins
CREATE TABLE group_users (
    group_id UUID NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permission TEXT NOT NULL DEFAULT 'member' CHECK (permission IN ('member', 'admin')),
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (group_id, user_id)
);
CREATE INDEX group_users_user_idx ON group_users (user_id);

CREATE TABLE collection_group_memberships (
    collection_id UUID NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    group_id UUID NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    permission TEXT NOT NULL CHECK (permission IN ('read', 'read_write')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (collection_id, group_id)
);
CREATE INDEX collection_group_memberships_group_idx ON collection_group_memberships (group_id);

-- Access to collections given to a user directly or through their groups; with several grants the
-- highest wins (max(permission): 'read_write' > 'read')
CREATE VIEW collection_grants AS
    SELECT collection_id, user_id, permission FROM collection_memberships
    UNION ALL
    SELECT cg.collection_id, gu.user_id, cg.permission
    FROM collection_group_memberships cg JOIN group_users gu ON gu.group_id = cg.group_id;

-- mentions of a group notify its members
ALTER TABLE notifications ADD COLUMN group_id UUID REFERENCES groups(id) ON DELETE SET NULL;
ALTER TABLE notifications DROP CONSTRAINT notifications_event_check;
ALTER TABLE notifications ADD CONSTRAINT notifications_event_check
    CHECK (event IN ('mentioned_in_document', 'group_mentioned_in_document'));
