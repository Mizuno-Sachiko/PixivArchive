UPDATE subscription
SET next_run_at = COALESCE(last_run_at, created_at)
    + make_interval(mins => (schedule ->> 'interval_minutes')::integer)
WHERE next_run_at IS NULL;

ALTER TABLE subscription
    ALTER COLUMN next_run_at SET NOT NULL;
