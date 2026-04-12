-- 002_timeseries.sql
-- TimescaleDB hypertables and continuous aggregates for sensor / meter readings.

BEGIN;

CREATE EXTENSION IF NOT EXISTS timescaledb;

-- ============================================================
-- Readings
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
-- Weather Observations
-- ============================================================
CREATE TABLE weather_observations (
    time                TIMESTAMPTZ NOT NULL,
    site_id             UUID NOT NULL REFERENCES sites(id),
    temperature_f       REAL,
    humidity_pct        REAL,
    wind_speed_mph      REAL,
    wind_direction_deg  REAL,
    solar_irradiance_wm2 REAL,
    pressure_inhg       REAL,
    conditions          TEXT
);

SELECT create_hypertable('weather_observations', 'time');

CREATE INDEX idx_weather_observations_site_time
    ON weather_observations(site_id, time DESC);

-- ============================================================
-- Continuous Aggregates
-- ============================================================

-- Hourly rollup
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

-- Daily rollup
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

COMMIT;
