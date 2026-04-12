# lothal_util Roadmap

## What Exists (v0.1 — CLI Foundation)

5-crate Rust workspace: lothal-core (ontology types), lothal-db (sqlx/TimescaleDB), lothal-ingest (bill parsers, MQTT, NWS, Flume, Ecobee), lothal-engine (baselines, simulation, recommendations, experiments), lothal-cli (full interactive CLI). 16k lines, 50 tests, zero warnings.

---

## Phase 2: Web Dashboard (lothal-web)

New crate: `crates/lothal-web/` using **Axum** for the HTTP API + **htmx** with server-rendered templates for the frontend. This keeps the stack in Rust, avoids a separate JS build, and htmx gives interactive feel with minimal complexity.

### API Layer (Axum)
- REST endpoints for all entities (CRUD for sites, devices, bills, etc.)
- JSON API for frontend consumption and future integrations
- WebSocket endpoint for real-time reading streams (from MQTT subscriber)
- Shared state: PgPool + app config passed via Axum state

### Dashboard Pages
- **Home / Overview** — site summary card, current month cost/usage snapshot, weather widget, active experiments count, top recommendation
- **Ontology Explorer** — interactive tree view of site > structures > zones > devices > circuits. Click any node to see details, readings, associated bills
- **Bills & Costs** — monthly bar chart of cost by utility type, stacked area chart of usage over time, table of all bills with drill-down to line items, month-over-month and year-over-year comparisons
- **Live Readings** — real-time charts (updating via WebSocket) for circuit-level power, water flow, HVAC runtime. Configurable time windows (1h, 24h, 7d, 30d)
- **Simulation Playground** — interactive "what if" forms: select a scenario type (device swap, rate change, setpoint shift, insulation upgrade), fill parameters with sliders/dropdowns, see projected savings live. Side-by-side current vs projected cost visualization
- **Experiments** — kanban-style board (planned / active / completed / inconclusive). Each card shows hypothesis, intervention, date ranges, and results. Click to see detailed pre/post weather-normalized charts
- **Recommendations** — prioritized cards with estimated savings, capex, payback period, confidence bar. Filter by category. "Start Experiment" button to convert a recommendation into an active experiment
- **Reports** — monthly and annual report generation with printable/PDF output

### Tech Details
- Templates: Askama (Jinja-like, compile-time checked Rust templates) or Maud (macro-based HTML)
- Charts: Chart.js via htmx partials, or Plotly.js for more complex visualizations
- Styling: Tailwind CSS (via CDN or standalone binary — no Node required)
- WebSocket: tokio-tungstenite for real-time reading pushes
- Auth: optional, single-user by default (personal tool), simple token or cookie if needed

---

## Phase 3: Enhanced Data Sources

### Home Assistant Integration
- Direct HA REST API integration (alternative to MQTT for simpler setup)
- Auto-discover HA entities and map to lothal devices/circuits
- Import historical data from HA's recorder database

### Smart Meter Direct Access
- OG&E Green Button Connect My Data (if they support OAuth-based access)
- Direct smart meter reading via RTL-SDR (some meters broadcast unencrypted AMR)

### Personal Weather Station
- Ecowitt or Tempest station integration (local API or cloud API)
- Higher fidelity than NWS for on-property microclimate

### Solar Monitoring (future-proofing)
- Enphase / SolarEdge API integration for production data
- Net metering cost calculations against rate schedules

---

## Phase 4: Smarter Analytics

### Machine Learning Baselines
- Replace simple linear regression with gradient-boosted models (using temperature, humidity, day-of-week, occupancy as features)
- Anomaly detection: flag unusual consumption patterns (leaks, stuck HVAC, phantom loads)
- Seasonal decomposition for more accurate year-over-year comparisons

### Automated Recommendations
- Triggered recommendations based on real-time data (e.g., "your pool pump has been running for 12 hours — consider a variable-speed upgrade")
- Cost alerts when projected monthly bill exceeds threshold
- TOU optimization alerts ("shift 3kWh of load from 2-5pm to save $X this month")

### Forecasting
- Monthly bill forecasting based on weather forecast + baseline model
- "What will this month's bill be?" with confidence intervals
- Budget planning: annual cost projection with seasonal patterns

---

## Phase 5: Automation & Notifications

### Home Assistant Automations
- Generate HA automation YAML from lothal recommendations (e.g., "schedule pool pump to run 11pm-7am" → HA automation)
- Closed-loop experiments: lothal creates the automation, monitors the result, evaluates automatically

### Notifications
- Slack/Discord/email alerts for: anomalies, experiment results, bill spikes, maintenance reminders
- Weekly/monthly digest emails with efficiency summary

### Scheduled Jobs
- Cron-based weather fetch, bill reminder, report generation
- systemd service for continuous MQTT ingest + WebSocket server

---

## Phase 6: Multi-Property & Sharing

### Multi-Site Support
- Already modeled (Site is top-level entity) but UI/CLI assumes single site
- Property comparison dashboards
- Portfolio-level analytics

### Export & Sharing
- Export data as CSV/JSON for external analysis
- Shareable reports (static HTML generation)
- Home energy audit document generation (for insurance, appraisals, or green certification)

---

## Non-Functional Improvements

### Testing
- Integration tests against a test database (Docker-based test harness)
- Property-based tests for bill parsers (generate random bill text, verify parser handles it)
- End-to-end CLI tests

### Observability
- Structured logging with tracing spans across all operations
- Metrics: ingest throughput, query latency, parser success/failure rates

### Deployment
- Single-binary release builds (cross-compile for x86_64 Linux)
- systemd unit files for lothal-web and lothal-ingest-mqtt
- Docker image for the full stack (web + worker + TimescaleDB)
- Nix flake for reproducible builds (matches Julian's Arch setup)
