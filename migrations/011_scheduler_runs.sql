-- Scheduler bookkeeping: track last-run timestamp per daemon task so restarts
-- resume cleanly and a single scheduler tick can dispatch only due work.

BEGIN;

CREATE TABLE scheduler_runs (
    task_name   TEXT PRIMARY KEY,
    last_run    TIMESTAMPTZ NOT NULL,
    last_status TEXT NOT NULL,
    last_error  TEXT,
    duration_ms INTEGER
);

COMMIT;
