-- Comments on a passage of the text, as in Outline. The passage is found again by its text and a
-- little context before and after it (the document's Markdown is not changed); when it no longer
-- exists the comment stays on the whole document, with the quoted text.
ALTER TABLE comments ADD COLUMN anchor_text TEXT;
ALTER TABLE comments ADD COLUMN anchor_prefix TEXT;
ALTER TABLE comments ADD COLUMN anchor_suffix TEXT;
