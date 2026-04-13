# lothal_util

Property operations ontology system. Models an entire property as a graph of physical entities (site, structures, zones, devices, circuits), land areas (property zones), water systems (sources, pools, septic), biological subsystems (flocks, garden beds, compost), financial entities (utility accounts, rate schedules, bills), time-series data (sensor readings, weather observations), and cross-system resource flows. Computes weather-normalized baselines for energy and water, tracks experiments, and generates property-wide efficiency recommendations.

The house is one subsystem. The pool, land, chickens, water cycle, and weather are the rest. Modeling them all in one schema means the cross-system questions — pool pump scheduling vs water use, chicken manure vs compost vs garden, septic load vs water flow, HVAC baselines vs weather — become answerable.

Built for a 1984 two-story on 0.89 acres in Guthrie, OK — but the ontology is general.

## Architecture

Rust workspace with eight crates:

| Crate | Purpose |
|---|---|
| `lothal-core` | Pure domain types — ontology entities (site, structures, devices, property zones, water systems, pools, septic, flocks, garden beds, resource flows), strongly-typed units (kWh, therms, gallons, USD), temporal helpers, CDD/HDD computation |
| `lothal-ontology` | Ontology layer — Object/Link/Event/Action primitives, `Describe` trait, transactional indexer, query composition (`get_object_view`, `neighbors`, `events_for`, `search`), `ActionRegistry` with six built-in actions, smoke tests |
| `lothal-db` | sqlx persistence layer — PostgreSQL + TimescaleDB, async CRUD, batch inserts, daily aggregation; every write-path repo emits ontology rows in the same transaction |
| `lothal-ingest` | Data pipelines — PDF bill parsers (OG&E, ONG, Guthrie water), Green Button XML, MQTT subscriber (Emporia Vue / Home Assistant), NWS weather API, Flume water meter, Ecobee thermostat |
| `lothal-engine` | Analytics — weather-normalized baselines, experiment evaluator, property-wide recommendation generator |
| `lothal-ai` | AI layer — LLM bill extraction, daily briefings (via ontology context), MCP server (six generic ontology tools + per-action tools from registry), NILM device identification |
| `lothal-cli` | CLI binary — onboarding, data management, querying, experiment tracking, recommendations, geometry import, ontology backfill |
| `lothal-web` | Web dashboard — Axum + Askama + htmx, dark theme; universal entity page (`/e/{kind}/{id}`), property map (`/map`), Pulse dashboard, bills view; entity-scoped tool-enabled chat; WebSocket live readings |

## Quick Start

```bash
# 1. Start the database
cp .env.example .env
docker compose up -d

# 2. Build
cargo build

# 3. Seed schema (site + utility accounts + circuits shell)
cargo run -- demo-seed

# 4. Add your first bill
cargo run -- bill add

# 5. See what you've got
cargo run -- site show
```

## Commands

```
lothal demo-seed                         Seed site schema (no fake data)
lothal init                              Interactive onboarding wizard
lothal site show                         Display ontology tree
lothal site edit                         Edit site properties

lothal device add                        Register a device
lothal device list                       List all devices
lothal device show <id>                  Device details

lothal bill add                          Enter a bill manually
lothal bill import <file>                Import PDF/CSV/XML bill
lothal bill list [account]               List bills

lothal ingest mqtt                       Start MQTT listener for sensors
lothal ingest weather [--days N]         Fetch NWS weather data

lothal query readings <device> [period]  Query sensor readings
lothal query bills <account> [year]      Query bill history

lothal baseline compute <account>        Compute weather-normalized baseline

lothal experiment create                 Create hypothesis + intervention
lothal experiment list                   List experiments
lothal experiment show <id>              Experiment details
lothal experiment evaluate <id>          Evaluate with weather normalization

lothal recommend                         Generate ranked recommendations

lothal property list                     List property zones
lothal property add-zone                 Add a property zone

lothal geometry import --site <id> --file <geojson>
                                         Import GeoJSON property boundaries

lothal ontology backfill [--dry-run]     Backfill objects/links/events from
                                         pre-existing domain rows

lothal water list                        List water sources, pools, septic
lothal water add-source                  Add a water source
lothal water add-pool                    Add a swimming pool
lothal water add-septic                  Add septic system

lothal livestock add-flock               Register a flock
lothal livestock show                    Show flock details
lothal livestock log                     Log daily event (eggs, feed, etc.)
lothal livestock list-logs [period]      List recent livestock logs

lothal garden list                       List garden beds and compost
lothal garden add-bed                    Add a garden bed
lothal garden add-planting               Record a planting
lothal garden add-compost                Add a compost pile

lothal report monthly <YYYY-MM>          Monthly efficiency report

lothal ai status                         Check LLM provider connectivity
lothal ai parse-bill <file>              Parse bill with LLM structured output
lothal ai briefing [--date D] [--output] Generate daily briefing
lothal ai mcp-server                     Start MCP server for reasoning agent
lothal ai ingest-email [--once]          Poll email for utility bill PDFs
lothal ai identify <circuit|all>         NILM device identification
```

## Web Dashboard

```bash
cargo run -p lothal-web
# Open http://localhost:3000
```

Three primary pages plus a universal entity drill-down:

- **Pulse** (`/`) — daily AI briefing, recent events stream, quick-action forms, stat cards (energy, cost, weather, eggs)
- **Map** (`/map`) — SVG property map from GeoJSON boundaries; click any feature to open the entity drawer
- **Bills** (`/bills`) — monthly stacked cost chart, bill table
- **Entity** (`/e/{kind}/{id}`) — Properties / Timeline / Graph (d3-force neighbors) / Actions / Chat panels for any ontology object

The entity Chat panel is scoped to the object and uses tool-enabled LLM with the full ontology tool catalog (`get_object`, `neighbors`, `events`, `timeline`, `search`, `run_action`).

## Ontology

### Domain graph

```
Site
 ├── Structure (1:N)
 │    ├── Zone (1:N) ── Device (N:M)
 │    └── Panel (1:N) ── Circuit (1:N) ── Device (N:1)
 ├── PropertyZone (1:N) ── outdoor lot areas (lawn, garden, coop, leach field, etc.)
 │    ├── Paddock (0:N) ── rotational grazing linked to Flock
 │    └── Constraint (M:N) ── restrictions (leach field, easement, setback)
 ├── UtilityAccount (1:N)
 │    ├── RateSchedule (1:N, temporal)
 │    └── Bill (1:N) ── BillLineItem (1:N)
 ├── WaterSource (1:N) ── municipal, well, cistern, rainwater
 ├── Pool (0:N)
 ├── SepticSystem (0:1)
 ├── Flock (0:N)
 │    └── LivestockLog (time-series)
 ├── GardenBed (0:N)
 │    └── Planting (0:N)
 ├── CompostPile (0:N)
 ├── ResourceFlow (time-series)
 ├── WeatherObservation (time-series)
 └── OccupancyEvent (time-series)

Reading ── source: Device | Circuit | Zone | Meter | PropertyZone | Pool | WeatherStation
MaintenanceEvent ── target: Device | Structure | PropertyZone | Pool | SepticSystem
Experiment ── Hypothesis + Intervention + DateRanges
Recommendation ── Site, optionally Device
```

### Ontology layer (lothal-ontology)

Every domain entity implements `Describe` (kind, id, display_name, properties). Repos write domain rows and ontology index rows in the same transaction. Four tables:

- **objects** — one row per entity, JSONB properties, full-text search vector
- **links** — typed, time-valid directed edges (`contained_in`, `issued_by`, `targets`, `powers`, …)
- **events** — TimescaleDB hypertable; one row per happening (`anomaly`, `observation`, `maintenance_scheduled`, `diagnosis`, `briefing_generated`, …)
- **action_runs** — audit log for every invoked action

### Built-in actions

| Action | Subjects | Description |
|---|---|---|
| `record_observation` | any | Log free-text human observation as an event |
| `schedule_maintenance` | device / structure / pool / zone | Insert maintenance event + emit event |
| `run_diagnostic` | circuit / device | Pull recent readings + anomalies → LLM root-cause hypothesis |
| `scoped_briefing` | any | LLM briefing filtered to entity's graph neighborhood |
| `apply_recommendation` | site / device | Create Experiment + Intervention from a recommendation |
| `ingest_bill_pdf` | utility_account | PDF → pdftotext → LLM extraction → bill rows |

## Data Sources

| Source | Integration | What It Provides |
|---|---|---|
| Utility bills (PDF) | `lothal bill import` | Monthly usage + cost by provider |
| OG&E portal (CSV/XML) | `lothal bill import` | Historical usage, Green Button interval data |
| Emporia Vue | MQTT via Home Assistant | Circuit-level power (watts/kWh) |
| Flume | REST API | Per-minute water flow |
| Ecobee | REST API | HVAC runtime, indoor/outdoor temp |
| NWS | REST API | Hourly weather observations |

## Database

PostgreSQL 17 + TimescaleDB. Schema lives in a single baseline migration (`migrations/001_baseline.sql`) applied at startup via `sqlx migrate`. It covers the full ontology in one pass: sites/structures/zones/panels/devices/circuits, utility accounts and bills, readings and weather hypertables with hourly/daily continuous aggregates, experiments and recommendations, property zones and constraints, water/livestock/garden/compost, cross-system resource flows, scheduler bookkeeping, anomaly alerts, AI-layer tables (briefings, NILM device labels, email ingest log), and the unified ontology primitives (objects, links, events, action_runs). Future schema changes land as new numbered migrations on top of this baseline.

Default port is 5433 (avoids conflict with other local Postgres instances).

## Requirements

- Rust 1.85+
- Docker (for TimescaleDB)
- `poppler` (`pdftotext` binary) for PDF bill parsing
- `ANTHROPIC_API_KEY` for AI features (briefings, diagnostics, bill extraction)

## License

MIT
