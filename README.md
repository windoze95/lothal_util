# lothal_util

Home efficiency ontology system. Models a property as a graph of physical entities (site, structures, zones, devices, circuits), financial entities (utility accounts, rate schedules, bills), and time-series data (sensor readings, weather observations). Computes weather-normalized baselines, runs "what if" simulations, tracks experiments, and generates efficiency recommendations.

Built for a 1984 two-story in Guthrie, OK — but the ontology is general.

## Architecture

Rust workspace with five crates:

| Crate | Purpose |
|---|---|
| `lothal-core` | Pure domain types — ontology entities, strongly-typed units (kWh, therms, gallons, USD), temporal helpers, CDD/HDD computation |
| `lothal-db` | sqlx persistence layer — PostgreSQL + TimescaleDB, async CRUD, batch inserts, daily weather aggregation |
| `lothal-ingest` | Data pipelines — PDF bill parsers (OG&E, ONG, Guthrie water), Green Button XML, CSV import, MQTT subscriber, NWS weather API, Flume water meter, Ecobee thermostat |
| `lothal-engine` | Analytics — weather-normalized baselines (linear regression), simulation engine (device swap, TOU shift, setpoint change, insulation upgrade), recommendation generator, experiment evaluator |
| `lothal-cli` | CLI binary — interactive onboarding wizard, data management, querying, simulation, experiment tracking, recommendations, reports |

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

lothal report monthly <YYYY-MM>          Monthly efficiency report
```

## Ontology

```
Site
 ├── Structure (1:N)
 │    ├── Zone (1:N) ── Device (N:M)
 │    └── Panel (1:N) ── Circuit (1:N) ── Device (N:1)
 ├── UtilityAccount (1:N)
 │    ├── RateSchedule (1:N, temporal)
 │    └── Bill (1:N) ── BillLineItem (1:N)
 ├── WeatherObservation (time-series)
 └── OccupancyEvent (time-series)

Reading ── source: Device | Circuit | Zone | Meter
MaintenanceEvent ── target: Device | Structure
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

PostgreSQL 17 + TimescaleDB. Three migrations:

1. **Core ontology** — 14 relational tables with UUID PKs
2. **Time-series** — hypertables for readings and weather, continuous aggregates (hourly/daily rollups)
3. **Experiments** — hypotheses, interventions, experiments, recommendations

Default port is 5433 (avoids conflict with other local Postgres instances).

## Requirements

- Rust 1.85+
- Docker (for TimescaleDB)
- `poppler` (`pdftotext` binary) for PDF bill parsing

## License

MIT
