-- Livestock subsystem: flocks, paddocks, and daily logs.

BEGIN;

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

COMMIT;
