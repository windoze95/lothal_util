-- Ontology core: unified Object / Link / Event / Action primitives.
-- Every domain row is indexed into `objects` and its edges into `links` (both via
-- transactional hooks in the domain repos).  Events are a TimescaleDB hypertable.

BEGIN;

CREATE TABLE objects (
    kind         TEXT NOT NULL,
    id           UUID NOT NULL,
    display_name TEXT NOT NULL,
    site_id      UUID,
    properties   JSONB NOT NULL,
    search_tsv   tsvector GENERATED ALWAYS AS (
                    to_tsvector('english',
                      coalesce(display_name,'') || ' ' ||
                      coalesce(properties->>'notes',''))
                  ) STORED,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at   TIMESTAMPTZ,
    PRIMARY KEY (kind, id)
);
CREATE INDEX idx_objects_site      ON objects (site_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_objects_kind      ON objects (kind)    WHERE deleted_at IS NULL;
CREATE INDEX idx_objects_search    ON objects USING gin(search_tsv);
CREATE INDEX idx_objects_props_gin ON objects USING gin(properties jsonb_path_ops);

CREATE TABLE links (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    kind          TEXT NOT NULL,
    src_kind      TEXT NOT NULL,
    src_id        UUID NOT NULL,
    dst_kind      TEXT NOT NULL,
    dst_id        UUID NOT NULL,
    valid_from    TIMESTAMPTZ NOT NULL DEFAULT now(),
    valid_until   TIMESTAMPTZ,
    properties    JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_links_src       ON links (src_kind, src_id, kind) WHERE valid_until IS NULL;
CREATE INDEX idx_links_dst       ON links (dst_kind, dst_id, kind) WHERE valid_until IS NULL;
CREATE INDEX idx_links_kind_time ON links (kind, valid_from DESC);
CREATE UNIQUE INDEX uq_links_current
    ON links (kind, src_kind, src_id, dst_kind, dst_id)
    WHERE valid_until IS NULL;

CREATE TABLE events (
    time         TIMESTAMPTZ NOT NULL,
    id           UUID NOT NULL DEFAULT gen_random_uuid(),
    kind         TEXT NOT NULL,
    site_id      UUID,
    subjects     JSONB NOT NULL,
    summary      TEXT NOT NULL,
    severity     TEXT,
    properties   JSONB NOT NULL DEFAULT '{}'::jsonb,
    source       TEXT NOT NULL
);
SELECT create_hypertable('events', 'time');
CREATE INDEX idx_events_site_time      ON events (site_id, time DESC);
CREATE INDEX idx_events_kind_time      ON events (kind, time DESC);
CREATE INDEX idx_events_subjects_gin   ON events USING gin(subjects jsonb_path_ops);

CREATE TABLE action_runs (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    action_name   TEXT NOT NULL,
    status        TEXT NOT NULL,
    invoked_by    TEXT NOT NULL,
    subjects      JSONB NOT NULL,
    input         JSONB NOT NULL,
    output        JSONB,
    error         TEXT,
    started_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at   TIMESTAMPTZ
);
CREATE INDEX idx_action_runs_name_time ON action_runs (action_name, started_at DESC);
CREATE INDEX idx_action_runs_subjects  ON action_runs USING gin(subjects jsonb_path_ops);

COMMIT;
