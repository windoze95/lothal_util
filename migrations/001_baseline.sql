-- 001_baseline.sql
-- Consolidated schema baseline for lothal_util. Greenfield — no prior data,
-- no prior migrations to honor. All future schema changes land in new
-- numbered migrations on top of this file.

BEGIN;

CREATE EXTENSION IF NOT EXISTS timescaledb;

-- ============================================================
-- Sites
-- ============================================================
CREATE TABLE sites (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    address         TEXT NOT NULL,
    city            TEXT NOT NULL,
    state           TEXT NOT NULL,
    zip             TEXT NOT NULL,
    latitude        DOUBLE PRECISION,
    longitude       DOUBLE PRECISION,
    lot_size_acres  DOUBLE PRECISION,
    climate_zone    TEXT,
    soil_type       TEXT,
    boundary        JSONB,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ============================================================
-- Structures
-- ============================================================
CREATE TABLE structures (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id         UUID NOT NULL REFERENCES sites(id),
    name            TEXT NOT NULL,
    year_built      INT,
    square_footage  DOUBLE PRECISION,
    stories         INT,
    foundation_type TEXT,
    has_pool        BOOLEAN NOT NULL DEFAULT false,
    pool_gallons    DOUBLE PRECISION,
    has_septic      BOOLEAN NOT NULL DEFAULT false,
    footprint       JSONB,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_structures_site_id ON structures(site_id);

-- ============================================================
-- Zones (HVAC zones within a structure)
-- ============================================================
CREATE TABLE zones (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    structure_id    UUID NOT NULL REFERENCES structures(id),
    name            TEXT NOT NULL,
    floor           INT,
    square_footage  DOUBLE PRECISION,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_zones_structure_id ON zones(structure_id);

-- ============================================================
-- Panels
-- ============================================================
CREATE TABLE panels (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    structure_id    UUID NOT NULL REFERENCES structures(id),
    name            TEXT NOT NULL,
    amperage        INT,
    is_main         BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_panels_structure_id ON panels(structure_id);

-- ============================================================
-- Devices (created before circuits so circuits can FK to it)
-- ============================================================
CREATE TABLE devices (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    structure_id            UUID NOT NULL REFERENCES structures(id),
    zone_id                 UUID REFERENCES zones(id),
    circuit_id              UUID,  -- FK added after circuits table exists
    name                    TEXT NOT NULL,
    kind                    TEXT NOT NULL,
    make                    TEXT,
    model                   TEXT,
    nameplate_watts         DOUBLE PRECISION,
    estimated_daily_hours   DOUBLE PRECISION,
    year_installed          INT,
    expected_lifespan_years INT,
    replacement_cost        DOUBLE PRECISION,
    notes                   TEXT,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_devices_structure_id ON devices(structure_id);
CREATE INDEX idx_devices_zone_id ON devices(zone_id);

-- ============================================================
-- Circuits
-- ============================================================
CREATE TABLE circuits (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    panel_id        UUID NOT NULL REFERENCES panels(id),
    breaker_number  INT NOT NULL,
    label           TEXT NOT NULL,
    amperage        INT NOT NULL,
    is_double_pole  BOOLEAN NOT NULL DEFAULT false,
    device_id       UUID REFERENCES devices(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_circuits_panel_id ON circuits(panel_id);
CREATE INDEX idx_circuits_device_id ON circuits(device_id);

ALTER TABLE devices
    ADD CONSTRAINT fk_devices_circuit_id
    FOREIGN KEY (circuit_id) REFERENCES circuits(id);

CREATE INDEX idx_devices_circuit_id ON devices(circuit_id);

-- ============================================================
-- Utility Accounts
-- ============================================================
CREATE TABLE utility_accounts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id         UUID NOT NULL REFERENCES sites(id),
    provider_name   TEXT NOT NULL,
    utility_type    TEXT NOT NULL,
    account_number  TEXT,
    meter_id        TEXT,
    is_active       BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_utility_accounts_site_id ON utility_accounts(site_id);

-- ============================================================
-- Rate Schedules
-- ============================================================
CREATE TABLE rate_schedules (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id      UUID NOT NULL REFERENCES utility_accounts(id),
    name            TEXT NOT NULL,
    rate_type       TEXT NOT NULL,
    effective_from  DATE NOT NULL,
    effective_until DATE,
    base_charge     DOUBLE PRECISION NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_rate_schedules_account_id ON rate_schedules(account_id);

CREATE UNIQUE INDEX idx_rate_schedules_active_per_account
    ON rate_schedules(account_id)
    WHERE effective_until IS NULL;

-- ============================================================
-- Rate Tiers
-- ============================================================
CREATE TABLE rate_tiers (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    schedule_id     UUID NOT NULL REFERENCES rate_schedules(id),
    label           TEXT NOT NULL,
    lower_limit     DOUBLE PRECISION NOT NULL,
    upper_limit     DOUBLE PRECISION,
    rate_per_unit   DOUBLE PRECISION NOT NULL,
    peak_hours      TEXT
);

CREATE INDEX idx_rate_tiers_schedule_id ON rate_tiers(schedule_id);

-- ============================================================
-- Bills
-- ============================================================
CREATE TABLE bills (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id      UUID NOT NULL REFERENCES utility_accounts(id),
    period_start    DATE NOT NULL,
    period_end      DATE NOT NULL,
    statement_date  DATE NOT NULL,
    due_date        DATE,
    total_usage     DOUBLE PRECISION NOT NULL,
    usage_unit      TEXT NOT NULL,
    total_amount    DOUBLE PRECISION NOT NULL,
    source_file     TEXT,
    notes           TEXT,
    parse_method    TEXT,
    llm_model       TEXT,
    llm_confidence  DOUBLE PRECISION,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT uq_bills_account_period UNIQUE (account_id, period_start, period_end)
);

CREATE INDEX idx_bills_account_id ON bills(account_id);

-- ============================================================
-- Bill Line Items
-- ============================================================
CREATE TABLE bill_line_items (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bill_id         UUID NOT NULL REFERENCES bills(id) ON DELETE CASCADE,
    description     TEXT NOT NULL,
    category        TEXT NOT NULL,
    amount          DOUBLE PRECISION NOT NULL,
    usage           DOUBLE PRECISION,
    rate            DOUBLE PRECISION
);

CREATE INDEX idx_bill_line_items_bill_id ON bill_line_items(bill_id);

-- ============================================================
-- Maintenance Events
-- ============================================================
CREATE TABLE maintenance_events (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    target_type     TEXT NOT NULL,
    target_id       UUID NOT NULL,
    date            DATE NOT NULL,
    event_type      TEXT NOT NULL,
    description     TEXT NOT NULL,
    cost            DOUBLE PRECISION,
    provider        TEXT,
    next_due        DATE,
    notes           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_maintenance_events_target ON maintenance_events(target_type, target_id);

-- ============================================================
-- Occupancy Events
-- ============================================================
CREATE TABLE occupancy_events (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id         UUID NOT NULL REFERENCES sites(id),
    start_time      TIMESTAMPTZ NOT NULL,
    end_time        TIMESTAMPTZ,
    occupant_count  INT NOT NULL,
    status          TEXT NOT NULL,
    notes           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_occupancy_events_site_id ON occupancy_events(site_id);

-- ============================================================
-- Time-Series: Readings
-- ============================================================
CREATE TABLE readings (
    time        TIMESTAMPTZ NOT NULL,
    source_type TEXT NOT NULL,
    source_id   UUID NOT NULL,
    kind        TEXT NOT NULL,
    value       DOUBLE PRECISION NOT NULL,
    metadata    JSONB
);

SELECT create_hypertable('readings', 'time');

CREATE INDEX idx_readings_source_time
    ON readings(source_id, time DESC);

CREATE INDEX idx_readings_kind_time
    ON readings(kind, time DESC);

CREATE INDEX idx_readings_source_type_id_time
    ON readings(source_type, source_id, time DESC);

-- ============================================================
-- Time-Series: Weather Observations
-- ============================================================
CREATE TABLE weather_observations (
    time                 TIMESTAMPTZ NOT NULL,
    site_id              UUID NOT NULL REFERENCES sites(id),
    temperature_f        REAL,
    humidity_pct         REAL,
    wind_speed_mph       REAL,
    wind_direction_deg   REAL,
    solar_irradiance_wm2 REAL,
    pressure_inhg        REAL,
    conditions           TEXT,
    source               TEXT NOT NULL DEFAULT 'nws',
    station_id           UUID,
    rainfall_inches      DOUBLE PRECISION
);

SELECT create_hypertable('weather_observations', 'time');

CREATE INDEX idx_weather_observations_site_time
    ON weather_observations(site_id, time DESC);

-- ============================================================
-- Continuous Aggregates (hourly / daily readings rollup)
-- ============================================================
CREATE MATERIALIZED VIEW readings_hourly
WITH (timescaledb.continuous) AS
SELECT
    time_bucket('1 hour', time)  AS bucket,
    source_id,
    kind,
    avg(value)                   AS avg_value,
    max(value)                   AS max_value,
    sum(value)                   AS sum_value,
    count(*)                     AS sample_count
FROM readings
GROUP BY bucket, source_id, kind
WITH NO DATA;

CREATE MATERIALIZED VIEW readings_daily
WITH (timescaledb.continuous) AS
SELECT
    time_bucket('1 day', time)   AS bucket,
    source_id,
    kind,
    avg(value)                   AS avg_value,
    max(value)                   AS max_value,
    min(value)                   AS min_value,
    sum(value)                   AS sum_value,
    count(*)                     AS sample_count
FROM readings
GROUP BY bucket, source_id, kind
WITH NO DATA;

-- ============================================================
-- Hypotheses
-- ============================================================
CREATE TABLE hypotheses (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id                 UUID NOT NULL REFERENCES sites(id),
    title                   TEXT NOT NULL,
    description             TEXT NOT NULL,
    expected_savings_pct    DOUBLE PRECISION,
    expected_savings_usd    DOUBLE PRECISION,
    category                TEXT NOT NULL,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_hypotheses_site_id ON hypotheses(site_id);

-- ============================================================
-- Interventions
-- ============================================================
CREATE TABLE interventions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id         UUID NOT NULL REFERENCES sites(id),
    device_id       UUID REFERENCES devices(id),
    description     TEXT NOT NULL,
    date_applied    DATE NOT NULL,
    cost            DOUBLE PRECISION,
    reversible      BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_interventions_site_id ON interventions(site_id);
CREATE INDEX idx_interventions_device_id ON interventions(device_id);

-- ============================================================
-- Experiments
-- ============================================================
CREATE TABLE experiments (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id             UUID NOT NULL REFERENCES sites(id),
    hypothesis_id       UUID NOT NULL REFERENCES hypotheses(id),
    intervention_id     UUID NOT NULL REFERENCES interventions(id),
    baseline_start      DATE NOT NULL,
    baseline_end        DATE NOT NULL,
    result_start        DATE NOT NULL,
    result_end          DATE NOT NULL,
    status              TEXT NOT NULL DEFAULT 'planned',
    actual_savings_pct  DOUBLE PRECISION,
    actual_savings_usd  DOUBLE PRECISION,
    confidence          DOUBLE PRECISION,
    notes               TEXT,
    baseline_snapshot   JSONB,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_experiments_site_id ON experiments(site_id);
CREATE INDEX idx_experiments_hypothesis_id ON experiments(hypothesis_id);
CREATE INDEX idx_experiments_intervention_id ON experiments(intervention_id);

-- ============================================================
-- Recommendations
-- ============================================================
CREATE TABLE recommendations (
    id                          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id                     UUID NOT NULL REFERENCES sites(id),
    device_id                   UUID REFERENCES devices(id),
    title                       TEXT NOT NULL,
    description                 TEXT NOT NULL,
    category                    TEXT NOT NULL,
    estimated_annual_savings    DOUBLE PRECISION NOT NULL,
    estimated_capex             DOUBLE PRECISION NOT NULL,
    payback_years               DOUBLE PRECISION NOT NULL,
    confidence                  DOUBLE PRECISION NOT NULL DEFAULT 0.5,
    priority_score              DOUBLE PRECISION NOT NULL DEFAULT 0,
    data_requirements           TEXT,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_recommendations_site_id ON recommendations(site_id);
CREATE INDEX idx_recommendations_device_id ON recommendations(device_id);

-- ============================================================
-- Property Zones (lot subdivisions, distinct from HVAC zones)
-- ============================================================
CREATE TABLE property_zones (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id         UUID NOT NULL REFERENCES sites(id),
    name            TEXT NOT NULL,
    kind            TEXT NOT NULL,
    area_sqft       DOUBLE PRECISION,
    sun_exposure    TEXT,
    slope           TEXT,
    soil_type       TEXT,
    drainage        TEXT,
    notes           TEXT,
    shape           JSONB,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_property_zones_site_id ON property_zones(site_id);

-- ============================================================
-- Constraints (restrictions on property zones)
-- ============================================================
CREATE TABLE constraints (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id         UUID NOT NULL REFERENCES sites(id),
    kind            TEXT NOT NULL,
    description     TEXT NOT NULL,
    geometry        TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_constraints_site_id ON constraints(site_id);

CREATE TABLE constraint_zones (
    constraint_id   UUID NOT NULL REFERENCES constraints(id) ON DELETE CASCADE,
    zone_id         UUID NOT NULL REFERENCES property_zones(id) ON DELETE CASCADE,
    PRIMARY KEY (constraint_id, zone_id)
);

-- ============================================================
-- Water Systems
-- ============================================================
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

-- ============================================================
-- Livestock
-- ============================================================
CREATE TABLE flocks (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id             UUID NOT NULL REFERENCES sites(id),
    name                TEXT NOT NULL,
    breed               TEXT NOT NULL,
    bird_count          INTEGER NOT NULL,
    coop_zone_id        UUID REFERENCES property_zones(id),
    date_established    DATE,
    status              TEXT NOT NULL DEFAULT 'active',
    notes               TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_flocks_site_id ON flocks(site_id);

CREATE TABLE paddocks (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    flock_id            UUID NOT NULL REFERENCES flocks(id),
    property_zone_id    UUID NOT NULL REFERENCES property_zones(id),
    rotation_order      INTEGER NOT NULL,
    last_rested         DATE,
    rest_days_target    INTEGER,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_paddocks_flock_id ON paddocks(flock_id);

CREATE TABLE livestock_logs (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    flock_id        UUID NOT NULL REFERENCES flocks(id),
    date            DATE NOT NULL,
    event_kind      TEXT NOT NULL,
    quantity        DOUBLE PRECISION,
    unit            TEXT,
    cost            DOUBLE PRECISION,
    notes           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_livestock_logs_flock_id ON livestock_logs(flock_id);
CREATE INDEX idx_livestock_logs_date ON livestock_logs(flock_id, date);

-- ============================================================
-- Garden & Compost
-- ============================================================
CREATE TABLE garden_beds (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id                 UUID NOT NULL REFERENCES sites(id),
    property_zone_id        UUID REFERENCES property_zones(id),
    name                    TEXT NOT NULL,
    area_sqft               DOUBLE PRECISION,
    bed_type                TEXT NOT NULL,
    soil_amendments         TEXT,
    irrigation_source_id    UUID REFERENCES water_sources(id),
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_garden_beds_site_id ON garden_beds(site_id);

CREATE TABLE plantings (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bed_id                  UUID NOT NULL REFERENCES garden_beds(id),
    crop                    TEXT NOT NULL,
    variety                 TEXT,
    date_planted            DATE NOT NULL,
    date_harvested          DATE,
    yield_lbs               DOUBLE PRECISION,
    water_consumed_gallons  DOUBLE PRECISION,
    notes                   TEXT,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_plantings_bed_id ON plantings(bed_id);
CREATE INDEX idx_plantings_date ON plantings(date_planted);

CREATE TABLE compost_piles (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id             UUID NOT NULL REFERENCES sites(id),
    property_zone_id    UUID REFERENCES property_zones(id),
    name                TEXT NOT NULL,
    capacity_cuft       DOUBLE PRECISION,
    current_volume_cuft DOUBLE PRECISION,
    notes               TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_compost_piles_site_id ON compost_piles(site_id);

-- ============================================================
-- Resource Flows (directed water/energy/biomass/nutrient flows;
-- cross-system loop substrate)
-- ============================================================
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

SELECT create_hypertable('resource_flows', 'timestamp', if_not_exists => TRUE);

CREATE INDEX idx_resource_flows_site_type ON resource_flows(site_id, resource_type);
CREATE INDEX idx_resource_flows_source ON resource_flows(source_type, source_id);
CREATE INDEX idx_resource_flows_sink ON resource_flows(sink_type, sink_id);

-- ============================================================
-- Scheduler bookkeeping
-- ============================================================
CREATE TABLE scheduler_runs (
    task_name   TEXT PRIMARY KEY,
    last_run    TIMESTAMPTZ NOT NULL,
    last_status TEXT NOT NULL,
    last_error  TEXT,
    duration_ms INTEGER
);

-- ============================================================
-- Anomaly alerts
-- ============================================================
CREATE TABLE anomaly_alerts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id         UUID NOT NULL REFERENCES sites(id),
    source_type     TEXT NOT NULL,
    source_id       UUID NOT NULL,
    kind            TEXT NOT NULL,
    detected_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    value           DOUBLE PRECISION NOT NULL,
    baseline_value  DOUBLE PRECISION NOT NULL,
    deviation_pct   DOUBLE PRECISION NOT NULL,
    message         TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'detected',
    delivered_at    TIMESTAMPTZ,
    acknowledged_at TIMESTAMPTZ
);

CREATE INDEX idx_anomaly_alerts_site_detected
    ON anomaly_alerts(site_id, detected_at DESC);

CREATE INDEX idx_anomaly_alerts_dedupe
    ON anomaly_alerts(source_id, kind, detected_at DESC);

-- ============================================================
-- AI Layer: briefings, NILM device labels, email ingest log
-- ============================================================
CREATE TABLE briefings (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id     UUID NOT NULL REFERENCES sites(id),
    date        DATE NOT NULL,
    content     TEXT NOT NULL,
    context     JSONB,
    model       TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_briefings_site_date UNIQUE (site_id, date)
);

CREATE INDEX idx_briefings_site_date ON briefings(site_id, date DESC);

CREATE TABLE device_labels (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    circuit_id      UUID NOT NULL REFERENCES circuits(id),
    device_kind     TEXT NOT NULL,
    confidence      DOUBLE PRECISION NOT NULL,
    reasoning       TEXT,
    signature       JSONB NOT NULL,
    model           TEXT,
    is_confirmed    BOOLEAN NOT NULL DEFAULT false,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_device_labels_circuit ON device_labels(circuit_id);

CREATE TABLE email_ingest_log (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    message_id  TEXT NOT NULL UNIQUE,
    sender      TEXT NOT NULL,
    subject     TEXT,
    received_at TIMESTAMPTZ,
    bill_id     UUID REFERENCES bills(id),
    status      TEXT NOT NULL,
    error       TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ============================================================
-- Ontology: Objects / Links / Events / Action Runs
-- ============================================================
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
