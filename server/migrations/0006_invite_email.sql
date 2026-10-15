-- When the invitation email was last sent (NULL: never)
ALTER TABLE invites ADD COLUMN email_sent_at TIMESTAMPTZ;
