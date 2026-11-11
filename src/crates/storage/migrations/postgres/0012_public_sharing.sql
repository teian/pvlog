-- Public sharing links for a system.
--
-- The token is an unguessable handle, separate from the system identifier, so a
-- link can be revoked and reissued without changing anything the API uses
-- internally. Sharing stays off until an operator turns it on.

ALTER TABLE management.system_registry
    ADD COLUMN public_token TEXT,
    ADD COLUMN link_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    -- A link cannot be enabled without a token to reach it by.
    ADD CONSTRAINT system_registry_public_link_check
        CHECK (NOT link_enabled OR public_token IS NOT NULL);

CREATE UNIQUE INDEX system_registry_public_token_idx
    ON management.system_registry(public_token) WHERE public_token IS NOT NULL;
