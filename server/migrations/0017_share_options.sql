-- Options of public links, as in Outline: the nested documents can be read through the link too,
-- search engines may index it, and the date of the last change can be hidden. Existing links keep
-- working as before (only the document, not indexed, date shown).
ALTER TABLE shares ADD COLUMN include_children BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE shares ADD COLUMN allow_indexing BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE shares ADD COLUMN show_last_updated BOOLEAN NOT NULL DEFAULT true;
