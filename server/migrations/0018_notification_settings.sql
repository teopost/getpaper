-- Email preferences per kind of notification, as in Outline: { "<event>": false } turns the
-- email off, a missing event means on. They replace email_on_mention and email_on_subscription,
-- which are carried over and left in place for older versions.
ALTER TABLE users ADD COLUMN notification_settings JSONB NOT NULL DEFAULT '{}';
UPDATE users SET notification_settings =
    (CASE WHEN email_on_mention THEN '{}'::jsonb ELSE '{"mentioned_in_document": false}'::jsonb END)
    || (CASE WHEN email_on_subscription THEN '{}'::jsonb
        ELSE '{"document_updated": false, "document_published": false, "document_shared": false}'::jsonb END);

-- notifications about a collection (access given) or a workspace (invitation accepted) have no document
ALTER TABLE notifications ALTER COLUMN document_id DROP NOT NULL;
ALTER TABLE notifications ADD COLUMN collection_id UUID REFERENCES collections(id) ON DELETE CASCADE;
ALTER TABLE notifications ADD COLUMN comment_id UUID REFERENCES comments(id) ON DELETE CASCADE;
ALTER TABLE notifications DROP CONSTRAINT notifications_event_check;
ALTER TABLE notifications ADD CONSTRAINT notifications_event_check CHECK (event IN (
    'mentioned_in_document', 'group_mentioned_in_document', 'document_updated', 'document_published', 'document_shared',
    'comment_created', 'comment_resolved', 'collection_added', 'invite_accepted'
));
