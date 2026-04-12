-- Water systems: sources, pools, septic, and directed water flows.

BEGIN;

-- ---------------------------------------------------------------------------
-- Water sources
-- ---------------------------------------------------------------------------

CREATE TABLE water_sources (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id             UUID NOT NULL REFERENCES sites(id),
    name                TEXT NOT NULL,
    kind                TEXT NOT NULL,
    capacity_gallons    DOUBLE PRECISION,
    flow_rate_gpm       DOUBLE PRECISION,
    cost_per_gallon     DOUBLE PRECISION,
    notes               TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_water_sources_site_id ON water_sources(site_id);

-- ---------------------------------------------------------------------------
-- Pools
-- ---------------------------------------------------------------------------

CREATE TABLE pools (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id             UUID NOT NULL REFERENCES sites(id),
    name                TEXT NOT NULL,
    volume_gallons      DOUBLE PRECISION NOT NULL,
    surface_area_sqft   DOUBLE PRECISION,
    pump_device_id      UUID REFERENCES devices(id),
    heater_device_id    UUID REFERENCES devices(id),
    cleaner_device_id   UUID REFERENCES devices(id),
    cover_type          TEXT,
    notes               TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_pools_site_id ON pools(site_id);

-- ---------------------------------------------------------------------------
-- Septic systems
-- ---------------------------------------------------------------------------

CREATE TABLE septic_systems (
    id                          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id                     UUID NOT NULL REFERENCES sites(id),
    tank_capacity_gallons       DOUBLE PRECISION,
    leach_field_zone_id         UUID REFERENCES property_zones(id),
    last_pumped                 DATE,
    pump_interval_months        INTEGER,
    daily_load_estimate_gallons DOUBLE PRECISION,
    notes                       TEXT,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_septic_systems_site_id ON septic_systems(site_id);

-- ---------------------------------------------------------------------------
-- Water flows (directed connections between entities)
-- ---------------------------------------------------------------------------

CREATE TABLE water_flows (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id         UUID NOT NULL REFERENCES sites(id),
    name            TEXT NOT NULL,
    source_type     TEXT NOT NULL,
    source_id       UUID NOT NULL,
    sink_type       TEXT NOT NULL,
    sink_id         UUID NOT NULL,
    flow_rate_gpm   DOUBLE PRECISION,
    is_active       BOOLEAN NOT NULL DEFAULT true,
    notes           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_water_flows_site_id ON water_flows(site_id);
CREATE INDEX idx_water_flows_source ON water_flows(source_type, source_id);
CREATE INDEX idx_water_flows_sink ON water_flows(sink_type, sink_id);

COMMIT;
