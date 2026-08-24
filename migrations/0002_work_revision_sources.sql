CREATE TABLE work_revision_source (
    id UUID PRIMARY KEY,
    work_revision_id UUID NOT NULL REFERENCES work_revision(id) ON DELETE CASCADE,
    subscription_id UUID REFERENCES subscription(id) ON DELETE SET NULL,
    subscription_run_id UUID REFERENCES subscription_run(id) ON DELETE SET NULL,
    subscription_name TEXT NOT NULL CHECK (length(btrim(subscription_name)) > 0),
    pixiv_user_id BIGINT NOT NULL CHECK (pixiv_user_id > 0),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT work_revision_source_run_unique
        UNIQUE (work_revision_id, subscription_run_id)
);

CREATE INDEX work_revision_source_revision_idx
    ON work_revision_source (
        work_revision_id,
        recorded_at DESC,
        subscription_name,
        id DESC
    );

CREATE INDEX work_revision_source_subscription_idx
    ON work_revision_source (subscription_id);

CREATE INDEX work_revision_source_run_idx
    ON work_revision_source (subscription_run_id);
