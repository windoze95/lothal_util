# lothal_util

Property operations ontology system. Models an entire property as a graph of physical entities (site, structures, zones, devices, circuits), land areas (property zones, trees, constraints), water systems (sources, pools, septic), biological subsystems (flocks, paddocks, garden beds, compost), financial entities (utility accounts, rate schedules, bills), time-series data (sensor readings, weather observations), and cross-system resource flows. Computes weather-normalized baselines for energy and water, runs "what if" simulations, tracks experiments, and generates property-wide efficiency recommendations.

The house is one subsystem. The pool, land, trees, chickens, water cycle, and weather are the rest. Modeling them all in one schema means the cross-system questions — pool coverage vs evaporation vs pump scheduling, tree shade vs HVAC load, chicken manure vs compost vs garden, septic load vs water use — become answerable.

Built for a 1984 two-story on 0.89 acres in Guthrie, OK — but the ontology is general.

## Architecture

Rust workspace with seven crates:

| Crate | Purpose |
|---|---|
| `lothal-core` | Pure domain types — ontology entities (site, structures, devices, property zones, trees, water systems, pools, septic, flocks, paddocks, garden beds, compost, resource flows), strongly-typed units (kWh, therms, gallons, USD, pounds, inches, ppm), temporal helpers, CDD/HDD computation |
| `lothal-db` | sqlx persistence layer — PostgreSQL + TimescaleDB, async CRUD, batch inserts, daily weather aggregation, property operations repos |
| `lothal-ingest` | Data pipelines — PDF bill parsers (OG&E, ONG, Guthrie water), Green Button XML, CSV import, MQTT subscriber, NWS weather API, Flume water meter, Ecobee thermostat |
| `lothal-engine` | Analytics — weather-normalized baselines for energy and water, simulation engine (device swap, TOU shift, setpoint change, cistern, pool cover, tree removal, flock expansion), property-wide recommendation generator (13 templates), experiment evaluator |
| `lothal-ai` | AI layer — LLM bill parsing with structured output, daily property operations briefings, MCP reasoning agent (14 tools), NILM device identification |
| `lothal-cli` | CLI binary — interactive onboarding wizard, data management, property zones, water systems, livestock tracking, garden management, querying, simulation, experiment tracking, recommendations, reports, AI commands |
| `lothal-web` | Web dashboard — Axum + Askama + htmx dark-theme dashboard with 8 pages (Pulse/Energy/Water/Property/Land/Lab/Bills/Chat), Chart.js visualizations, WebSocket real-time readings, LLM-powered chat |

## Quick Start

```bash
# 1. Start the database
cp .env.example .env
docker compose up -d

# 2. Build
cargo build

# 3. Initialize your home
cargo run -- init

# 4. Add your first bill
cargo run -- bill add

# 5. See what you've got
cargo run -- site show
```

## Commands

```
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

lothal simulate swap-pump <hp>           Pool pump upgrade simulation
lothal simulate rate-change <plan>       Rate schedule comparison
lothal simulate setpoint <delta> <season> Thermostat adjustment model

lothal experiment create                 Create hypothesis + intervention
lothal experiment list                   List experiments
lothal experiment show <id>              Experiment details
lothal experiment evaluate <id>          Evaluate with weather normalization

lothal recommend                         Generate ranked recommendations

lothal property list                     List property zones, trees, constraints
lothal property add-zone                 Add a property zone
lothal property add-tree                 Add a tree
lothal property add-constraint           Add a constraint (leach field, easement, etc.)

lothal water list                        List water sources, pools, septic
lothal water add-source                  Add a water source (municipal, well, cistern)
lothal water add-pool                    Add a swimming pool
lothal water add-septic                  Add septic system

lothal livestock add-flock               Register a flock
lothal livestock show                    Show flock details and paddocks
lothal livestock log                     Log daily event (eggs, feed, etc.)
lothal livestock list-logs [period]      List recent livestock logs

lothal garden list                       List garden beds and compost piles
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

Eight pages accessible via sidebar navigation:

- **Pulse** — AI briefing, stat cards, alerts, experiments, top recommendation
- **Energy** — usage chart (24h/7d/30d/1y), circuit breakdown, baseline model, live power
- **Water** — pool status, septic pump-out countdown
- **Property** — interactive SVG zone map
- **Land** — livestock (egg/feed tracking), garden beds
- **Lab** — recommendations ranked by ROI, experiment kanban, simulations
- **Bills** — monthly stacked bar chart, bill table
- **Chat** — natural language queries via LLM

## Ontology

```
Site
 ├── Structure (1:N)
 │    ├── Zone (1:N) ── Device (N:M)
 │    └── Panel (1:N) ── Circuit (1:N) ── Device (N:1)
 ├── PropertyZone (1:N) ── outdoor lot areas (lawn, garden, coop, leach field, etc.)
 │    ├── Tree (0:N) ── species, canopy, shade analysis, cooling value
 │    ├── Paddock (0:N) ── rotational grazing linked to Flock
 │    └── Constraint (M:N) ── restrictions (leach field, easement, setback)
 ├── UtilityAccount (1:N)
 │    ├── RateSchedule (1:N, temporal)
 │    └── Bill (1:N) ── BillLineItem (1:N)
 ├── WaterSource (1:N) ── municipal, well, cistern, rainwater
 ├── Pool (0:N) ── volume, surface area, pump/heater/cover
 ├── SepticSystem (0:1) ── tank, leach field zone, pump schedule
 ├── Flock (0:N) ── breed, bird count, coop zone
 │    ├── Paddock (1:N) ── rotation order, rest schedule
 │    └── LivestockLog (time-series) ── eggs, feed, water, manure, events
 ├── GardenBed (0:N) ── type, area, irrigation source
 │    └── Planting (0:N) ── crop, dates, yield
 ├── CompostPile (0:N) ── capacity, volume, fill tracking
 ├── WaterFlow (0:N) ── directed water connections between entities
 ├── ResourceFlow (time-series) ── cross-system flows (water, energy, biomass, nutrients)
 ├── WeatherObservation (time-series, NWS or on-property)
 └── OccupancyEvent (time-series)

Reading ── source: Device | Circuit | Zone | Meter | PropertyZone | Pool | WeatherStation
MaintenanceEvent ── target: Device | Structure | PropertyZone | Pool | Tree | SepticSystem
Experiment ── Hypothesis + Intervention + DateRanges
Recommendation ── Site, optionally Device
```

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

PostgreSQL 17 + TimescaleDB. Ten migrations:

1. **Core ontology** — 14 relational tables with UUID PKs
2. **Time-series** — hypertables for readings and weather, continuous aggregates (hourly/daily rollups)
3. **Experiments** — hypotheses, interventions, experiments, recommendations
4. **AI layer** — bill parse provenance, daily briefings, NILM device labels, email ingest log
5. **Property zones** — property zones, constraints, constraint-zone junction, trees
6. **Water systems** — water sources, pools, septic systems, water flows
7. **Livestock** — flocks, paddocks, livestock logs
8. **Garden** — garden beds, plantings, compost piles
9. **Resource flows** — cross-system resource flow hypertable
10. **Microclimate** — weather observation source tracking and rainfall

Default port is 5433 (avoids conflict with other local Postgres instances).

## Requirements

- Rust 1.85+
- Docker (for TimescaleDB)
- `poppler` (`pdftotext` binary) for PDF bill parsing

## License

MIT
