-- 001_core_ontology.sql
-- Core ontology tables for the home efficiency system.

BEGIN;

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
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_structures_site_id ON structures(site_id);

-- ============================================================
-- Zones
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

-- Now add the deferred FK from devices → circuits.
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

-- At most one active (open-ended) schedule per account.
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

COMMIT;
