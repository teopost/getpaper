-- Links between documents of a workspace, for the "Referenced by" list (backlinks), as in Outline.
-- Kept up to date by a trigger whenever the text of a document changes, whatever changes it:
-- mentions of documents (mention://<id>/document/<document id>) and links to /doc/<slug>-<url id>.
CREATE TABLE document_links (
    source_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    target_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    PRIMARY KEY (source_id, target_id)
);
CREATE INDEX document_links_target_idx ON document_links (target_id);

-- the documents of workspace `p_team` that `p_text` links to, except `p_source` itself (the parameters
-- have a prefix: in SQL functions column names, like documents.text, win over parameter names)
CREATE FUNCTION document_link_targets(p_source UUID, p_team UUID, p_text TEXT) RETURNS SETOF UUID
LANGUAGE sql STABLE AS $$
    SELECT d.id FROM documents d
    WHERE d.team_id = p_team AND d.id <> p_source AND d.url_id IN (
        SELECT m[1] FROM regexp_matches(p_text, '/doc/(?:[^\s()<>"#?/]*-)?([A-Za-z0-9]{10})(?![A-Za-z0-9-])', 'g') AS m)
    UNION
    SELECT d.id FROM documents d
    WHERE d.team_id = p_team AND d.id <> p_source AND d.id::text IN (
        SELECT lower(m[1]) FROM regexp_matches(p_text, 'mention://[A-Za-z0-9_-]+/document/([0-9a-fA-F-]{36})', 'g') AS m)
$$;

CREATE FUNCTION document_links_sync() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM document_links WHERE source_id = NEW.id;
    INSERT INTO document_links (source_id, target_id)
    SELECT DISTINCT NEW.id, t FROM document_link_targets(NEW.id, NEW.team_id, NEW.text) AS t
    ON CONFLICT DO NOTHING;
    RETURN NEW;
END
$$;

CREATE TRIGGER documents_links_insert AFTER INSERT ON documents
    FOR EACH ROW EXECUTE FUNCTION document_links_sync();
CREATE TRIGGER documents_links_update AFTER UPDATE OF text, team_id ON documents
    FOR EACH ROW WHEN (OLD.text IS DISTINCT FROM NEW.text OR OLD.team_id IS DISTINCT FROM NEW.team_id)
    EXECUTE FUNCTION document_links_sync();

-- links of the documents that already exist
INSERT INTO document_links (source_id, target_id)
SELECT DISTINCT d.id, t FROM documents d, LATERAL document_link_targets(d.id, d.team_id, d.text) AS t
ON CONFLICT DO NOTHING;
