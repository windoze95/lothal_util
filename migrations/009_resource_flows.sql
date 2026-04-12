-- Resource flow graph: directed flows of water, energy, biomass, nutrients
-- between entities on the property. This is the cross-system loop substrate.

BEGIN;

CREATE TABLE resource_flows (
    id              UUID NOT NULL DEFAULT gen_random_uuid(),
    site_id         UUID NOT NULL REFERENCES sites(id),
    resource_type   TEXT NOT NULL,
    source_type     TEXT NOT NULL,
    source_id       UUID NOT NULL,
    sink_type       TEXT NOT NULL,
    sink_id         UUID NOT NULL,
    quantity        DOUBLE PRECISION NOT NULL,
    unit            TEXT NOT NULL,
    cost            DOUBLE PRECISION,
    timestamp       TIMESTAMPTZ NOT NULL,
    notes           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Make it a hypertable for time-series queries.
SELECT create_hypertable('resource_flows', 'timestamp', if_not_exists => TRUE);

CREATE INDEX idx_resource_flows_site_type ON resource_flows(site_id, resource_type);
CREATE INDEX idx_resource_flows_source ON resource_flows(source_type, source_id);
CREATE INDEX idx_resource_flows_sink ON resource_flows(sink_type, sink_id);

COMMIT;
