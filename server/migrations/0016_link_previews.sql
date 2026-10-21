-- Previews of external links (General settings → Link previews, off by default), kept for a day
CREATE TABLE link_previews (
    url TEXT PRIMARY KEY,
    data JSONB NOT NULL DEFAULT '{}',
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
