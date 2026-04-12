-- 003_experiments.sql
-- Hypotheses, interventions, experiments, and recommendations.

BEGIN;

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

COMMIT;
