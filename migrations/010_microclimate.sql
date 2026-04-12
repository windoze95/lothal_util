-- Microclimate expansion: add source tracking and rainfall to weather observations.

BEGIN;

ALTER TABLE weather_observations ADD COLUMN source TEXT NOT NULL DEFAULT 'nws';
ALTER TABLE weather_observations ADD COLUMN station_id UUID;
ALTER TABLE weather_observations ADD COLUMN rainfall_inches DOUBLE PRECISION;

COMMIT;
