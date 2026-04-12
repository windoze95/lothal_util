# lothal_util Roadmap

## What Exists (v0.2 — Property Operations)

6-crate Rust workspace expanded from "home efficiency" to full **property operations ontology**. The house is one subsystem; the lot, pool, water cycle, chickens, garden, compost, and weather are the rest. All joined in one schema so cross-system questions are answerable.

- lothal-core: 16 ontology modules, 20+ entity types, 50+ enum types, 12 strongly-typed units
- lothal-db: 10 PostgreSQL/TimescaleDB migrations, 13 repository modules
- lothal-ingest: bill parsers (OG&E, ONG, Guthrie), MQTT, NWS, Flume, Ecobee
- lothal-engine: energy + water baselines, 8 simulation scenarios, 13 recommendation templates, experiment evaluator
- lothal-ai: LLM bill parsing, property operations daily briefings, MCP reasoning agent (14 tools), NILM
- lothal-cli: 17 command groups, 55+ subcommands

~22k lines, 138 tests, zero new warnings. AI is a consumer of the ontology, not the load-bearing element.

---

## Phase 2: AI Layer (lothal-ai) -- IMPLEMENTED

New crate: `crates/lothal-ai/`. Three distinct AI surfaces, built in order of ROI.

### Guiding Principles

- **LLM for extraction + reasoning + narrative. Code for math, validation, detection, control.**
- If you can write the rule, write the rule. Don't ask a model to do arithmetic.
- "LLM as extractor, code as validator" — the model parses, deterministic functions verify.
- Ontology queries return real data. The agent reasons over results, not hallucinations.
- Local models for narrow/frequent tasks. Frontier models for complex/rare reasoning.

### 2a. Ingest Agent (build first — highest ROI)

**Problem:** Every utility's PDF is a snowflake. Regex parsers break when they redesign the layout. The existing regex parsers in `lothal-ingest/src/bill/` work for known formats but are brittle and took significant effort.

**Solution:** Replace regex bill parsing with LLM structured output extraction. One prompt with a `Bill` schema handles OG&E, ONG, Guthrie water, insurance declarations, and any future provider — and survives format changes.

Implementation:
- `fn parse_bill_with_llm(text: &str, account_id: Uuid) -> Result<Bill, IngestError>`
- Extract PDF text with pdftotext (keep this — it's reliable)
- Send text + structured output schema to Claude API (or local model)
- Schema: `{ period_start, period_end, total_usage, usage_unit, total_amount, line_items: [{ description, category, amount }] }`
- **Validate with code:** line items must sum to total (within $0.02). Reject and retry if they don't.
- Keep regex parsers as fallback — if LLM extraction fails or is unavailable, fall back to the deterministic path
- Scheduled mode: pull new bills from email (IMAP), parse, validate, write to Postgres. Cron-driven, boring, reliable.
- **Model choice:** Local model (Gemma on starforge) is fine for this narrow extraction task. Cheap, fast, runs forever. Claude as fallback for tricky bills.

### 2b. Daily Briefing (build second — immediate daily value)

One prompt, runs every morning, looks at yesterday's data:
- Compare usage to baseline (already computed by lothal-engine)
- Flag anything anomalous (SQL detection) and **explain** it (LLM reasoning)
- Stitch weather + occupancy + circuit data + maintenance log into 3-5 sentences a human can act on
- Drop into Home Assistant notification, Slack, or stdout

Example output:
> Yesterday: 47.2 kWh ($5.18), 12% above baseline. CDD was 18 vs 14-day avg of 15, so weather explains about half. The other half: Circuit 7 (pool pump) ran 9.5 hours vs the usual 6 — check the timer schedule. HVAC filter was last changed 87 days ago (recommended: 90). No anomalies on water.

**Model choice:** Cheap/local. The reasoning is shallow — it's summarizing structured data, not doing novel analysis.

### 2c. Reasoning Agent (build last — needs months of data)

The Foundry/AIP piece. An agent that sits behind an **MCP server** exposing tools for:
- Querying the ontology (bills by period, readings by device/time, weather data)
- Running statistical functions (baseline computation, normalization)
- Looking up rate schedules and computing costs
- Proposing `Hypothesis` and `Experiment` objects and writing them to the DB
- Evaluating completed experiments with weather normalization

Use cases:
- **"What should I do this month?"** — agent queries bills, readings, weather, identifies highest-leverage interventions, proposes experiments
- **"Why was my July bill so high?"** — agent pulls bills, weather, compares to baseline, checks circuit data, occupancy, explains in plain English
- **"What did the pool pump cost me last month vs the month before?"** — natural language query that would be painful as ad-hoc SQL
- **Maintenance reasoning** — "HVAC running 18% longer per CDD than last August. Filter due? Coils dirty? Refrigerant low?" Agent checks maintenance history, performance trends, typical failure modes, gives ranked differential with cheapest test first
- **Hypothesis generation** — "Find the three highest-leverage interventions for reducing summer cooling cost" → agent runs queries, reasons over results, proposes Hypothesis objects with designed Experiments

**Model choice:** Frontier model (Claude), used sparingly — this is complex multi-source reasoning.

**MCP server shape:**
```
tools:
  query_bills(account_id?, start?, end?) -> Bill[]
  query_readings(source_id, kind, start, end) -> Reading[]
  query_weather(site_id, start, end) -> WeatherObservation[]
  get_devices(structure_id?) -> Device[]
  get_rate_schedule(account_id) -> RateSchedule
  compute_baseline(account_id, mode) -> BaselineModel
  simulate(scenario) -> SimulationResult
  create_hypothesis(title, description, category) -> Hypothesis
  create_experiment(hypothesis_id, intervention, periods) -> Experiment
  evaluate_experiment(experiment_id) -> ExperimentEvaluation
```

### 2d. Device Identification (NILM)

Emporia gives per-circuit watts but doesn't know what's running. Classical signature-matching is brittle. An LLM that sees "Circuit 14, 4200W for 38min then 800W for 12min, weekday afternoon" can label it and explain its reasoning so you can correct it.

- Build a labeled training set from the first few weeks of manual identification
- Run inference on new patterns, write device attribution to readings metadata
- **Model choice:** Local model (Gemma on starforge). Narrow task, runs frequently, needs to be cheap.

### What AI Does NOT Do

- **Real-time control loops** — that's a thermostat. LLM lives in the planning layer, not execution.
- **Anomaly detection** — `kWh > 1.5 * 30d_avg AND temp < 95F` is a SQL query. Write the rule.
- **Bill math** — LLM extracts, code validates. Models confidently round things.
- **Forecasting** — Prophet/ARIMA/seasonal-naive beats an LLM on time-series and costs nothing.

---

## Phase 2.5: Property Operations Expansion -- IMPLEMENTED

Expanded the ontology from "home efficiency" to "property operations". Six new migrations, six new core entity modules, five new repo modules, four new CLI command groups, expanded engine/AI/MCP.

### New Subsystems
- **Property spatial model** — PropertyZone (13 kinds), Constraint (7 kinds), Tree with shade/cooling analysis
- **Water systems** — WaterSource (municipal, well, cistern, rainwater), Pool (first-class entity with pump/heater/cover), SepticSystem (pump scheduling, lifespan tracking), WaterFlow (directed connections)
- **Livestock** — Flock, Paddock (rotational grazing), LivestockLog (daily events: eggs, feed, water, manure, predator incidents)
- **Garden & compost** — GardenBed (5 types), Planting (crop tracking with yield), CompostPile (volume tracking)
- **Resource flows** — Cross-system flow graph (FlowEndpoint: 10 polymorphic variants) tracking water, energy, biomass, nutrients between any entities
- **Microclimate** — On-property weather station support, rainfall tracking

### Cross-System Integration
- ReadingKind expanded with 11 new variants (soil, pool chemistry, livestock, compost)
- DeviceKind expanded with 10 new variants (coop, irrigation, weather sensors)
- MaintenanceTarget/Type expanded for property operations
- HypothesisCategory expanded: WaterConservation, LivestockOptimization, LandManagement
- 6 new recommendation templates: pool cover, rainwater capture, tree shade, septic, coop efficiency, garden drip
- 4 new simulation scenarios: cistern install, pool cover, tree removal, flock expansion
- Water baseline regression (usage vs temperature)
- Briefing context includes pool, livestock, septic alerts
- MCP tools: get_property_zones, get_pool_status, query_livestock_logs, get_property_overview

---

## Phase 3: Web Dashboard (lothal-web)

New crate: `crates/lothal-web/` using **Axum** + **htmx** with server-rendered templates.

### API Layer (Axum)
- REST endpoints for all entities (CRUD for sites, devices, bills, etc.)
- JSON API for frontend consumption and future integrations
- WebSocket endpoint for real-time reading streams (from MQTT subscriber)
- Shared state: PgPool + app config passed via Axum state

### Dashboard Pages
- **Home / Overview** — site summary card, current month snapshot, weather, active experiments, top recommendation, daily briefing text
- **Ontology Explorer** — interactive tree: site > structures > zones > devices > circuits
- **Bills & Costs** — monthly bar/stacked-area charts, drill-down to line items, month-over-month and YoY comparisons
- **Live Readings** — real-time charts via WebSocket, configurable time windows
- **Simulation Playground** — interactive "what if" forms with sliders, side-by-side cost visualization
- **Experiments** — kanban board (planned / active / completed / inconclusive), pre/post weather-normalized charts
- **Recommendations** — prioritized cards with ROI, "Start Experiment" button
- **Chat** — natural language interface to the reasoning agent (Phase 2c)
- **Reports** — monthly/annual generation, printable output

### Tech
- Templates: Askama or Maud (compile-time checked)
- Charts: Chart.js or Plotly.js
- Styling: Tailwind CSS (standalone binary, no Node)
- WebSocket: tokio-tungstenite
- Auth: optional, single-user default

---

## Phase 4: Enhanced Data Sources

### Home Assistant Integration
- Direct HA REST API (alternative to MQTT)
- Auto-discover HA entities → map to lothal devices/circuits
- Import historical data from HA recorder

### Personal Weather Station
- Ecowitt or Tempest integration (local or cloud API)
- Higher fidelity than NWS for on-property microclimate

### Smart Meter Direct Access
- OG&E Green Button Connect My Data (OAuth)
- RTL-SDR for unencrypted AMR meters

### Solar Monitoring (future-proofing)
- Enphase / SolarEdge API
- Net metering cost calculations

---

## Phase 5: Automation & Notifications

### Home Assistant Automations
- Generate HA automation YAML from recommendations ("schedule pool pump 11pm-7am")
- Closed-loop experiments: lothal creates automation, monitors result, evaluates automatically

### Notifications
- Daily briefing → HA notification / Slack / email (wired to Phase 2b)
- Alerts: anomalies, experiment results, bill spikes, maintenance reminders
- Weekly/monthly digest

### Scheduled Jobs
- Cron: weather fetch, bill ingest from email, daily briefing, report generation
- systemd services for MQTT ingest + web server

---

## Phase 6: Smarter Analytics (non-AI)

### Statistical Forecasting
- Prophet / ARIMA / seasonal-naive for bill forecasting
- "What will this month's bill be?" with confidence intervals
- Annual cost projection with seasonal patterns

### Improved Baselines
- Multi-feature regression (temperature, humidity, day-of-week, occupancy)
- Seasonal decomposition for YoY comparisons

### Anomaly Detection Rules
- Configurable SQL-based rules with thresholds
- Alert integration (feeds into Phase 5 notifications)
- Anomaly explanations powered by reasoning agent (Phase 2c)

---

## Phase 7: Multi-Property & Sharing

- Multi-site dashboards and portfolio analytics
- Export: CSV/JSON/static HTML reports
- Home energy audit document generation

---

## Non-Functional

### Testing
- Integration tests against Docker test DB
- Property-based tests for parsers
- End-to-end CLI tests

### Deployment
- Single-binary release builds
- systemd unit files
- Docker image (web + worker + TimescaleDB)
- Nix flake
