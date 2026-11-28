-- Secret-manager convergence: replace legacy SSH-session ciphertext with a
-- canonical encrypted-secret reference. The legacy password_enc column remains
-- only as a one-time migration source for existing databases.
ALTER TABLE ssh_sessions
    ADD COLUMN password_secret_id TEXT;
