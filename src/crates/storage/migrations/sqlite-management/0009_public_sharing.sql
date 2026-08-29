-- Public sharing links for a system.
--
-- The token is an unguessable handle, separate from the system identifier, so a
-- link can be revoked and reissued without changing anything the API uses
-- internally. Sharing stays off until an operator turns it on.

ALTER TABLE system_registry ADD COLUMN public_token TEXT;
ALTER TABLE system_registry ADD COLUMN link_enabled INTEGER NOT NULL DEFAULT 0
    CHECK (link_enabled IN (0, 1));

CREATE UNIQUE INDEX system_registry_public_token_idx
    ON system_registry(public_token) WHERE public_token IS NOT NULL;

-- A link cannot be enabled without a token to reach it by.
CREATE TRIGGER system_registry_public_token_insert_guard
BEFORE INSERT ON system_registry
WHEN NEW.link_enabled = 1 AND NEW.public_token IS NULL
BEGIN
    SELECT RAISE(ABORT, 'an enabled sharing link requires a token');
END;

CREATE TRIGGER system_registry_public_token_update_guard
BEFORE UPDATE OF public_token, link_enabled ON system_registry
WHEN NEW.link_enabled = 1 AND NEW.public_token IS NULL
BEGIN
    SELECT RAISE(ABORT, 'an enabled sharing link requires a token');
END;
