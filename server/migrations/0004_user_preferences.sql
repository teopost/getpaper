-- Personal language and appearance. NULL means "use the installation default"
-- (DEFAULT_LANGUAGE / DEFAULT_THEME / DEFAULT_CONTENT_WIDTH).
ALTER TABLE users ADD COLUMN language TEXT CHECK (language IN ('en', 'it'));
ALTER TABLE users ADD COLUMN theme TEXT CHECK (theme IN ('system', 'light', 'dark'));

-- content width now also falls back to the installation default; "normal" was the implicit default so far
ALTER TABLE users ALTER COLUMN content_width DROP NOT NULL, ALTER COLUMN content_width DROP DEFAULT;
UPDATE users SET content_width = NULL WHERE content_width = 'normal';
