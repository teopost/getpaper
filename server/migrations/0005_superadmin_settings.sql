-- Superadmins manage the settings shared by every workspace of the installation
ALTER TABLE users ADD COLUMN is_superadmin BOOLEAN NOT NULL DEFAULT false;
-- the first account of an existing installation becomes superadmin
UPDATE users SET is_superadmin = true WHERE id = (SELECT id FROM users ORDER BY created_at LIMIT 1);

-- Installation settings, one JSON document per key (currently only "instance")
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value JSONB NOT NULL,
    updated_by UUID REFERENCES users(id) ON DELETE SET NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
