-- Microsoft Entra ID (Azure AD) single sign-on
ALTER TABLE users ADD COLUMN microsoft_id TEXT;
CREATE UNIQUE INDEX users_microsoft_id_idx ON users (microsoft_id) WHERE microsoft_id IS NOT NULL;

-- Users signing in with SSO whose email domain is listed here join the workspace automatically
ALTER TABLE teams ADD COLUMN allowed_domains TEXT NOT NULL DEFAULT '';
