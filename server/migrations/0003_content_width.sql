-- Per-user preference for the width of the document text column
ALTER TABLE users ADD COLUMN content_width TEXT NOT NULL DEFAULT 'normal'
    CHECK (content_width IN ('normal', 'wide', 'full'));
