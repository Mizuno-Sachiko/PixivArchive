ALTER TABLE app_event
    ADD COLUMN published_id BIGINT;

UPDATE app_event
SET published_id = id;

CREATE TABLE app_event_publication_cursor (
    singleton BOOLEAN PRIMARY KEY DEFAULT true CHECK (singleton),
    last_published_id BIGINT NOT NULL CHECK (last_published_id >= 0)
);

INSERT INTO app_event_publication_cursor (singleton, last_published_id)
SELECT true, COALESCE(max(published_id), 0)
FROM app_event;

DROP INDEX app_event_replay_idx;
DROP INDEX app_event_resource_idx;

CREATE UNIQUE INDEX app_event_replay_idx
    ON app_event (published_id)
    WHERE published_id IS NOT NULL;
CREATE INDEX app_event_resource_idx
    ON app_event (resource, resource_id, published_id)
    WHERE published_id IS NOT NULL;
