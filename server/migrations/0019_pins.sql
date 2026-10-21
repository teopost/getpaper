-- Documents pinned at the top of the home page (collection_id NULL) or of a collection, in an
-- order chosen by who pins them, as in Outline
CREATE TABLE pins (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    collection_id UUID REFERENCES collections(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX pins_home_idx ON pins (team_id, document_id) WHERE collection_id IS NULL;
CREATE UNIQUE INDEX pins_collection_idx ON pins (collection_id, document_id) WHERE collection_id IS NOT NULL;
