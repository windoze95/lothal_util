-- Phase 2: AI Layer
-- Adds provenance tracking for LLM-parsed bills, daily briefing history,
-- NILM device labels, and email ingest tracking.

-- ---------------------------------------------------------------------------
-- Bill parse provenance
-- ---------------------------------------------------------------------------

ALTER TABLE bills ADD COLUMN parse_method TEXT;
ALTER TABLE bills ADD COLUMN llm_model TEXT;
ALTER TABLE bills ADD COLUMN llm_confidence DOUBLE PRECISION;

-- ---------------------------------------------------------------------------
-- Daily briefings
-- ---------------------------------------------------------------------------

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

-- ---------------------------------------------------------------------------
-- NILM device labels
-- ---------------------------------------------------------------------------

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

-- ---------------------------------------------------------------------------
-- Email ingest tracking
-- ---------------------------------------------------------------------------

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
