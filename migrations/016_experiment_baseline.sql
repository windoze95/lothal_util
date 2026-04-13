-- 016_experiment_baseline.sql
-- Adds an opaque `baseline_snapshot` JSONB column to `experiments` so actions
-- that spin up an experiment from a recommendation can capture a pre-change
-- usage snapshot (e.g. mean kWh/day) for later comparison.
--
-- Keeping the shape JSONB (rather than a normalized side table) matches the
-- current design intent: the snapshot is written once at experiment creation
-- and read back only when the experiment is evaluated. A separate table would
-- add indirection without unlocking any meaningful queryability.

BEGIN;

ALTER TABLE experiments
    ADD COLUMN baseline_snapshot JSONB;

COMMIT;
