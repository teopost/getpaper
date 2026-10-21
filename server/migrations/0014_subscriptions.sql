-- Following documents and collections, as in Outline: notifications when a followed document
-- changes or a document is published in a followed collection. People who publish or edit a
-- document follow it automatically; unfollowing keeps the row (active = false) so editing again
-- doesn't follow it again.
CREATE TABLE subscriptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    document_id UUID REFERENCES documents(id) ON DELETE CASCADE,
    collection_id UUID REFERENCES collections(id) ON DELETE CASCADE,
    active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT subscriptions_one_target CHECK ((document_id IS NULL) <> (collection_id IS NULL))
);
CREATE UNIQUE INDEX subscriptions_user_document_idx ON subscriptions (user_id, document_id) WHERE document_id IS NOT NULL;
CREATE UNIQUE INDEX subscriptions_user_collection_idx ON subscriptions (user_id, collection_id) WHERE collection_id IS NOT NULL;
CREATE INDEX subscriptions_document_idx ON subscriptions (document_id) WHERE active;
CREATE INDEX subscriptions_collection_idx ON subscriptions (collection_id) WHERE active;

-- personal preference: also send an email for followed documents and documents shared with me
ALTER TABLE users ADD COLUMN email_on_subscription BOOLEAN NOT NULL DEFAULT true;

ALTER TABLE notifications DROP CONSTRAINT notifications_event_check;
ALTER TABLE notifications ADD CONSTRAINT notifications_event_check CHECK (event IN (
    'mentioned_in_document', 'group_mentioned_in_document', 'document_updated', 'document_published', 'document_shared'
));
