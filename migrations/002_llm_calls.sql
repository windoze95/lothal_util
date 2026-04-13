-- 002_llm_calls.sql
--
-- Trace table for every LLM invocation routed through LlmFunctionRegistry.
-- Mirrors the shape of action_runs (pending/running/succeeded/failed) and
-- adds the metadata needed for per-function routing, prompt versioning, and
-- cost accounting:
--
--   - tier + model:           what actually ran
--   - prompt_hash (sha256):   content-addressed version, cheap diff surface
--   - tokens_in/out:          cost & usage accounting
--   - latency_ms:             observability
--   - parent_action_run_id:   links to action_runs when an Action triggered
--                             the LLM call (e.g. run_diagnostic)
--   - thread_id:              nullable hook for future conversation threads;
--                             stays NULL for stateless entity-scoped chat

BEGIN;

CREATE TABLE llm_calls (
    id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    function_name         TEXT NOT NULL,
    status                TEXT NOT NULL,
    invoked_by            TEXT NOT NULL,
    tier                  TEXT NOT NULL,
    prompt_hash           TEXT NOT NULL,
    model                 TEXT,
    input                 JSONB NOT NULL,
    output                JSONB,
    error                 TEXT,
    tokens_in             INTEGER,
    tokens_out            INTEGER,
    latency_ms            BIGINT,
    parent_action_run_id  UUID REFERENCES action_runs(id) ON DELETE SET NULL,
    thread_id             UUID,
    started_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at           TIMESTAMPTZ
);

CREATE INDEX idx_llm_calls_function_time  ON llm_calls (function_name, started_at DESC);
CREATE INDEX idx_llm_calls_prompt_hash    ON llm_calls (prompt_hash);
CREATE INDEX idx_llm_calls_parent_action  ON llm_calls (parent_action_run_id)
    WHERE parent_action_run_id IS NOT NULL;
CREATE INDEX idx_llm_calls_thread         ON llm_calls (thread_id)
    WHERE thread_id IS NOT NULL;

COMMIT;
