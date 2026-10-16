-- In-app notifications; for now one kind: someone mentioned the user in a document
CREATE TABLE notifications (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    actor_id UUID REFERENCES users(id) ON DELETE SET NULL,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    event TEXT NOT NULL CHECK (event IN ('mentioned_in_document')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    viewed_at TIMESTAMPTZ,
    emailed_at TIMESTAMPTZ
);
CREATE INDEX notifications_user_idx ON notifications (user_id, team_id, created_at DESC);

-- Personal preference: also send an email when mentioned
ALTER TABLE users ADD COLUMN email_on_mention BOOLEAN NOT NULL DEFAULT true;
