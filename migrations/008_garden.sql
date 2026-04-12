-- Garden and compost: beds, plantings, and compost piles.

BEGIN;

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

COMMIT;
