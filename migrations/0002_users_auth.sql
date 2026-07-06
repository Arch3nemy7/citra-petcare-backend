CREATE TYPE user_role AS ENUM ('VET', 'ADMIN');

-- Clinic staff. There is no public registration: rows are created by the seed
-- command (or manually). IDs are UUIDv7 and may be supplied by the client.
CREATE TABLE users (
    id            uuid PRIMARY KEY,
    name          text        NOT NULL,
    email         text        NOT NULL,
    password_hash text        NOT NULL,
    role          user_role   NOT NULL DEFAULT 'VET',
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    deleted_at    timestamptz
);

-- Unique among non-deleted rows so a soft-deleted account frees its email.
CREATE UNIQUE INDEX users_email_key ON users (lower(email)) WHERE deleted_at IS NULL;

CREATE TRIGGER users_set_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Rotating refresh tokens. Only a SHA-256 hash of the token is stored; the
-- plaintext exists once, in the login/refresh response. This table is an auth
-- artifact, not a synced domain entity, so it uses revoked_at instead of the
-- soft-delete convention.
CREATE TABLE refresh_tokens (
    id         uuid PRIMARY KEY,
    user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash text        NOT NULL,
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX refresh_tokens_token_hash_key ON refresh_tokens (token_hash);
CREATE INDEX refresh_tokens_user_id_idx ON refresh_tokens (user_id);
