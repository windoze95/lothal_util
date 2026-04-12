-- Property spatial model: zones, constraints, and trees.
-- PropertyZone represents areas of the lot (distinct from HVAC zones in structures).

BEGIN;

-- ---------------------------------------------------------------------------
-- Property zones
-- ---------------------------------------------------------------------------

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
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_property_zones_site_id ON property_zones(site_id);

-- ---------------------------------------------------------------------------
-- Constraints (restrictions on property zones)
-- ---------------------------------------------------------------------------

CREATE TABLE constraints (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id         UUID NOT NULL REFERENCES sites(id),
    kind            TEXT NOT NULL,
    description     TEXT NOT NULL,
    geometry        TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_constraints_site_id ON constraints(site_id);

-- Junction table: which zones are affected by which constraints.
CREATE TABLE constraint_zones (
    constraint_id   UUID NOT NULL REFERENCES constraints(id) ON DELETE CASCADE,
    zone_id         UUID NOT NULL REFERENCES property_zones(id) ON DELETE CASCADE,
    PRIMARY KEY (constraint_id, zone_id)
);

-- ---------------------------------------------------------------------------
-- Trees
-- ---------------------------------------------------------------------------

CREATE TABLE trees (
    id                          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id                     UUID NOT NULL REFERENCES sites(id),
    property_zone_id            UUID REFERENCES property_zones(id),
    species                     TEXT NOT NULL,
    common_name                 TEXT,
    canopy_radius_ft            DOUBLE PRECISION,
    height_ft                   DOUBLE PRECISION,
    health                      TEXT NOT NULL DEFAULT 'unknown',
    distance_to_structure_ft    DOUBLE PRECISION,
    shade_direction             TEXT,
    estimated_cooling_value_usd DOUBLE PRECISION,
    notes                       TEXT,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_trees_site_id ON trees(site_id);
CREATE INDEX idx_trees_property_zone_id ON trees(property_zone_id);

COMMIT;
