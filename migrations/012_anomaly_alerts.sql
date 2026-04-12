-- Anomaly alerts: detected events that may warrant user attention. Used by the
-- anomaly sweep to dedupe (don't re-alert the same source within 24h unless
-- the deviation grows by >20%) and to let users acknowledge alerts.

BEGIN;

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

COMMIT;
