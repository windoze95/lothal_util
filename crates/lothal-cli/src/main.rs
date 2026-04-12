mod commands;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lothal", about = "Home efficiency ontology system")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new site with guided onboarding
    Init,
    /// Manage sites, structures, and zones
    Site {
        #[command(subcommand)]
        command: SiteCommands,
    },
    /// Manage devices and circuits
    Device {
        #[command(subcommand)]
        command: DeviceCommands,
    },
    /// Manage utility bills
    Bill {
        #[command(subcommand)]
        command: BillCommands,
    },
    /// Data ingestion commands
    Ingest {
        #[command(subcommand)]
        command: IngestCommands,
    },
    /// Query data
    Query {
        #[command(subcommand)]
        command: QueryCommands,
    },
    /// Compute weather-normalized baselines
    Baseline {
        #[command(subcommand)]
        command: BaselineCommands,
    },
    /// Run "what if" simulations
    Simulate {
        #[command(subcommand)]
        command: SimulateCommands,
    },
    /// Manage experiments
    Experiment {
        #[command(subcommand)]
        command: ExperimentCommands,
    },
    /// Generate efficiency recommendations
    Recommend,
    /// Generate reports
    Report {
        #[command(subcommand)]
        command: ReportCommands,
    },
    /// AI-powered features
    Ai {
        #[command(subcommand)]
        command: AiCommands,
    },
    /// Manage property zones and constraints
    Property {
        #[command(subcommand)]
        command: PropertyCommands,
    },
    /// Manage water sources, pools, and septic
    Water {
        #[command(subcommand)]
        command: WaterCommands,
    },
    /// Manage flocks, paddocks, and livestock logs
    Livestock {
        #[command(subcommand)]
        command: LivestockCommands,
    },
    /// Manage garden beds, plantings, and compost
    Garden {
        #[command(subcommand)]
        command: GardenCommands,
    },
    /// Import property geography (GeoJSON boundaries, footprints, zone shapes)
    Geometry {
        #[command(subcommand)]
        command: GeometryCommands,
    },
    /// Run the scheduler daemon (weather pull, email ingest, anomaly sweep, daily briefing)
    Daemon,
    /// Seed a Guthrie-shaped demo dataset so the web dashboard renders meaningfully on day 1
    DemoSeed,
}

// ---------------------------------------------------------------------------
// Site
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum SiteCommands {
    /// Display the full site ontology tree
    Show,
    /// Interactively edit site properties
    Edit,
}

// ---------------------------------------------------------------------------
// Device
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum DeviceCommands {
    /// Register a new device interactively
    Add,
    /// List all registered devices
    List,
    /// Show details for a specific device
    Show {
        /// Device ID (full UUID or short prefix)
        id: String,
    },
}

// ---------------------------------------------------------------------------
// Bill
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum BillCommands {
    /// Enter a bill manually
    Add,
    /// Import a bill from a file (PDF, CSV, XML)
    Import {
        /// Path to the bill file
        path: String,
    },
    /// List bills for one or all accounts
    List {
        /// Filter by account (provider name, type, or account number)
        account: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// Ingest
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum IngestCommands {
    /// Start MQTT listener for real-time device readings
    Mqtt,
    /// Fetch weather data from NWS
    Weather {
        /// Number of days to fetch
        #[arg(default_value = "7")]
        days: u32,
    },
}

// ---------------------------------------------------------------------------
// Query
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum QueryCommands {
    /// Query device readings
    Readings {
        /// Device name or ID
        device: String,
        /// Time window (e.g., "24h", "7d", "30d")
        #[arg(default_value = "24h")]
        last: String,
    },
    /// Query bills for an account
    Bills {
        /// Account provider name or ID
        account: String,
        /// Filter to a specific year
        year: Option<i32>,
    },
}

// ---------------------------------------------------------------------------
// Baseline
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum BaselineCommands {
    /// Compute a weather-normalized baseline for an account
    Compute {
        /// Account provider name or ID
        account: String,
    },
}

// ---------------------------------------------------------------------------
// Simulate
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum SimulateCommands {
    /// Simulate a thermostat setpoint change
    Setpoint {
        /// Degrees of change (+/-)
        change: f64,
        /// Season: "summer" or "winter"
        season: String,
    },
}

// ---------------------------------------------------------------------------
// Experiment
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum ExperimentCommands {
    /// Create a new experiment
    Create,
    /// List all experiments
    List,
    /// Show details of an experiment
    Show {
        /// Experiment ID
        id: String,
    },
    /// Evaluate an experiment's results
    Evaluate {
        /// Experiment ID
        id: String,
    },
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum ReportCommands {
    /// Generate a monthly efficiency report
    Monthly {
        /// Month in YYYY-MM format
        month: String,
    },
}

// ---------------------------------------------------------------------------
// AI
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum AiCommands {
    /// Check LLM provider connectivity
    Status,
    /// Parse a bill using LLM structured output
    ParseBill {
        /// Path to the PDF file
        path: String,
        /// Override LLM provider (ollama, anthropic)
        #[arg(long)]
        provider: Option<String>,
    },
    /// Generate a daily briefing
    Briefing {
        /// Date to analyze (default: yesterday)
        #[arg(long)]
        date: Option<String>,
        /// Output target: stdout, ha, slack
        #[arg(long, default_value = "stdout")]
        output: String,
    },
    /// Start MCP server for the reasoning agent
    McpServer,
    /// Poll email for new utility bills
    IngestEmail {
        /// Run once and exit (vs continuous polling)
        #[arg(long)]
        once: bool,
    },
    /// Run NILM device identification on circuits
    Identify {
        /// Circuit UUID or "all"
        circuit: String,
        /// Time window to analyze (e.g., "7d", "30d")
        #[arg(default_value = "7d")]
        window: String,
    },
}

// ---------------------------------------------------------------------------
// Property
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum PropertyCommands {
    /// List property zones and constraints
    List,
    /// Add a property zone
    AddZone,
    /// Add a constraint
    AddConstraint,
}

// ---------------------------------------------------------------------------
// Water
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum WaterCommands {
    /// List water sources, pools, and septic
    List,
    /// Add a water source
    AddSource,
    /// Add a pool
    AddPool,
    /// Add septic system
    AddSeptic,
}

// ---------------------------------------------------------------------------
// Livestock
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum LivestockCommands {
    /// Add a new flock
    AddFlock,
    /// Show flock details and paddocks
    Show,
    /// Log a daily event (eggs, feed, etc.)
    Log,
    /// List recent logs
    ListLogs {
        /// Time window (e.g., "7d", "30d")
        #[arg(default_value = "7d")]
        last: String,
    },
}

// ---------------------------------------------------------------------------
// Garden
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum GardenCommands {
    /// List garden beds and compost piles
    List,
    /// Add a garden bed
    AddBed,
    /// Record a planting
    AddPlanting,
    /// Add a compost pile
    AddCompost,
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum GeometryCommands {
    /// Import a GeoJSON FeatureCollection file and update site/structure/zone geometry
    Import {
        /// Site UUID (used when a feature's target is `site_boundary`)
        #[arg(long)]
        site: String,
        /// Path to the GeoJSON FeatureCollection file
        #[arg(long)]
        file: String,
    },
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file (ignore errors if not present)
    let _ = dotenvy::dotenv();

    // Set up tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    // Parse CLI args first (so --help works without DB)
    let cli = Cli::parse();

    // Create database pool
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set (in .env or environment)");
    let pool = lothal_db::create_pool(&database_url).await?;

    // Run migrations
    lothal_db::run_migrations(&pool).await?;

    match cli.command {
        Commands::Init => {
            commands::init::run_init(&pool).await?;
        }

        Commands::Site { command } => match command {
            SiteCommands::Show => commands::site::show_site(&pool).await?,
            SiteCommands::Edit => commands::site::edit_site(&pool).await?,
        },

        Commands::Device { command } => match command {
            DeviceCommands::Add => commands::device::add_device(&pool).await?,
            DeviceCommands::List => commands::device::list_devices(&pool).await?,
            DeviceCommands::Show { id } => {
                commands::device::show_device(&pool, &id).await?
            }
        },

        Commands::Bill { command } => match command {
            BillCommands::Add => commands::bill::add_bill(&pool).await?,
            BillCommands::Import { path } => {
                commands::bill::import_bill(&pool, &path).await?
            }
            BillCommands::List { account } => {
                commands::bill::list_bills(&pool, account.as_deref()).await?
            }
        },

        Commands::Ingest { command } => match command {
            IngestCommands::Mqtt => {
                commands::ingest::run_mqtt_ingest(&pool).await?;
            }
            IngestCommands::Weather { days } => {
                commands::ingest::fetch_weather(&pool, days).await?;
            }
        },

        Commands::Query { command } => match command {
            QueryCommands::Readings { device, last } => {
                commands::query::query_readings(&pool, &device, &last).await?;
            }
            QueryCommands::Bills { account, year } => {
                commands::query::query_bills(&pool, &account, year).await?;
            }
        },

        Commands::Baseline { command } => match command {
            BaselineCommands::Compute { account } => {
                commands::baseline::compute_baseline_cmd(&pool, &account).await?;
            }
        },

        Commands::Simulate { command } => match command {
            SimulateCommands::Setpoint { change, season } => {
                commands::simulate::simulate_setpoint(&pool, change, &season).await?;
            }
        },

        Commands::Experiment { command } => match command {
            ExperimentCommands::Create => {
                commands::experiment::create_experiment(&pool).await?;
            }
            ExperimentCommands::List => {
                commands::experiment::list_experiments(&pool).await?;
            }
            ExperimentCommands::Show { id } => {
                commands::experiment::show_experiment(&pool, &id).await?;
            }
            ExperimentCommands::Evaluate { id } => {
                commands::experiment::evaluate_experiment_cmd(&pool, &id).await?;
            }
        },

        Commands::Recommend => {
            commands::recommend::generate_recommendations(&pool).await?;
        }

        Commands::Report { command } => match command {
            ReportCommands::Monthly { month } => {
                commands::report::monthly_report(&pool, &month).await?;
            }
        },

        Commands::Ai { command } => match command {
            AiCommands::Status => {
                commands::ai::check_status().await?;
            }
            AiCommands::ParseBill { path, provider } => {
                commands::ai::parse_bill(&pool, &path, provider.as_deref()).await?;
            }
            AiCommands::Briefing { date, output } => {
                commands::ai::briefing(&pool, date.as_deref(), &output).await?;
            }
            AiCommands::McpServer => {
                commands::ai::mcp_server(pool).await?;
            }
            AiCommands::IngestEmail { once } => {
                commands::ai::ingest_email(&pool, once).await?;
            }
            AiCommands::Identify { circuit, window } => {
                commands::ai::identify(&pool, &circuit, &window).await?;
            }
        },

        Commands::Property { command } => match command {
            PropertyCommands::List => commands::property::list_zones(&pool).await?,
            PropertyCommands::AddZone => commands::property::add_zone(&pool).await?,
            PropertyCommands::AddConstraint => commands::property::add_constraint(&pool).await?,
        },

        Commands::Water { command } => match command {
            WaterCommands::List => commands::water::list_sources(&pool).await?,
            WaterCommands::AddSource => commands::water::add_source(&pool).await?,
            WaterCommands::AddPool => commands::water::add_pool(&pool).await?,
            WaterCommands::AddSeptic => commands::water::add_septic(&pool).await?,
        },

        Commands::Livestock { command } => match command {
            LivestockCommands::AddFlock => commands::livestock::add_flock(&pool).await?,
            LivestockCommands::Show => commands::livestock::show_flock(&pool).await?,
            LivestockCommands::Log => commands::livestock::log_event(&pool).await?,
            LivestockCommands::ListLogs { last } => {
                commands::livestock::list_logs(&pool, &last).await?
            }
        },

        Commands::Garden { command } => match command {
            GardenCommands::List => commands::garden::list_beds(&pool).await?,
            GardenCommands::AddBed => commands::garden::add_bed(&pool).await?,
            GardenCommands::AddPlanting => commands::garden::add_planting(&pool).await?,
            GardenCommands::AddCompost => commands::garden::add_compost_pile(&pool).await?,
        },

        Commands::Geometry { command } => match command {
            GeometryCommands::Import { site, file } => {
                commands::geometry::import(&pool, &site, &file).await?;
            }
        },

        Commands::Daemon => {
            commands::daemon::run(pool).await?;
        }

        Commands::DemoSeed => {
            commands::demo::seed(&pool).await?;
        }
    }

    Ok(())
}
