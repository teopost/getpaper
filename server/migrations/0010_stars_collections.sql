-- Stars on collections as well as documents, in an order chosen by the user (as in Outline)
ALTER TABLE stars DROP CONSTRAINT stars_pkey;
ALTER TABLE stars ADD COLUMN id UUID NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY;
ALTER TABLE stars ALTER COLUMN document_id DROP NOT NULL;
ALTER TABLE stars ADD COLUMN collection_id UUID REFERENCES collections(id) ON DELETE CASCADE;
ALTER TABLE stars ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stars ADD CONSTRAINT stars_one_target CHECK ((document_id IS NULL) <> (collection_id IS NULL));
CREATE UNIQUE INDEX stars_user_document_idx ON stars (user_id, document_id) WHERE document_id IS NOT NULL;
CREATE UNIQUE INDEX stars_user_collection_idx ON stars (user_id, collection_id) WHERE collection_id IS NOT NULL;

-- existing stars keep the order they had (oldest first)
UPDATE stars s SET position = n.rn
FROM (SELECT id, row_number() OVER (PARTITION BY user_id ORDER BY created_at) AS rn FROM stars) n
WHERE n.id = s.id;
