-- Property geography: GeoJSON geometry columns for sites, structures, and
-- property zones. Stored as JSONB (GeoJSON FeatureCollection features); no
-- PostGIS. One-time imports via `lothal geometry import`; spatial math is
-- handled client-side by the map renderer.

BEGIN;

ALTER TABLE sites          ADD COLUMN IF NOT EXISTS boundary  JSONB;
ALTER TABLE structures     ADD COLUMN IF NOT EXISTS footprint JSONB;
ALTER TABLE property_zones ADD COLUMN IF NOT EXISTS shape     JSONB;

COMMIT;
