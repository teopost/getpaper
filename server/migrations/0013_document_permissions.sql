-- Documents shared with people or groups beyond their collection, as in Outline. The permission
-- also applies to the nested documents; the highest one wins ('read_write' > 'read').
CREATE TABLE document_memberships (
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permission TEXT NOT NULL CHECK (permission IN ('read', 'read_write')),
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (document_id, user_id)
);
CREATE INDEX document_memberships_user_idx ON document_memberships (user_id);

CREATE TABLE document_group_memberships (
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    group_id UUID NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    permission TEXT NOT NULL CHECK (permission IN ('read', 'read_write')),
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (document_id, group_id)
);
CREATE INDEX document_group_memberships_group_idx ON document_group_memberships (group_id);

CREATE VIEW document_grants AS
    SELECT document_id, user_id, permission FROM document_memberships
    UNION ALL
    SELECT dg.document_id, gu.user_id, dg.permission
    FROM document_group_memberships dg JOIN group_users gu ON gu.group_id = dg.group_id;

-- Highest permission of a user on a document, given on it or on one of its parents; NULL without
-- any. Returns at once for people with no document grants at all, which is the common case.
CREATE FUNCTION document_grant(doc UUID, uid UUID) RETURNS TEXT LANGUAGE sql STABLE AS $$
    SELECT CASE WHEN NOT EXISTS (SELECT 1 FROM document_grants WHERE user_id = uid) THEN NULL ELSE (
        WITH RECURSIVE up AS (
            SELECT id, parent_document_id, 0 AS depth FROM documents WHERE id = doc
            UNION ALL
            SELECT d.id, d.parent_document_id, up.depth + 1 FROM documents d JOIN up ON d.id = up.parent_document_id
            WHERE up.depth < 100
        )
        SELECT max(g.permission) FROM document_grants g JOIN up ON up.id = g.document_id WHERE g.user_id = uid
    ) END
$$;
