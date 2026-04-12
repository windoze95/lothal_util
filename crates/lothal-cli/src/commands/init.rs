use anyhow::{Context, Result};
use dialoguer::{Confirm, Input, Select};
use sqlx::PgPool;

use lothal_core::ontology::site::{FoundationType, Site, SoilType, Structure, Zone};
use lothal_core::ontology::utility::{UtilityAccount, UtilityType};
use lothal_core::units::{Acres, SquareFeet};

/// Interactive onboarding wizard that walks the user through setting up their
/// entire home ontology: site, structure(s), zones, and utility accounts.
pub async fn run_init(pool: &PgPool) -> Result<()> {
    println!();
    println!("==========================================================");
    println!("  Welcome to Lothal -- Home Efficiency Ontology System");
    println!("==========================================================");
    println!();
    println!("This wizard will walk you through setting up your property.");
    println!("You can always edit these details later with `lothal site edit`.");
    println!();

    // -----------------------------------------------------------------------
    // 1. Site setup
    // -----------------------------------------------------------------------
    println!("--- Site Setup ---");
    println!();

    let address: String = Input::new()
        .with_prompt("Street address")
        .interact_text()
        .context("failed to read address")?;

    let city: String = Input::new()
        .with_prompt("City")
        .interact_text()
        .context("failed to read city")?;

    let state: String = Input::new()
        .with_prompt("State (2-letter code)")
        .interact_text()
        .context("failed to read state")?;

    let zip: String = Input::new()
        .with_prompt("ZIP code")
        .interact_text()
        .context("failed to read zip")?;

    let enter_coords = Confirm::new()
        .with_prompt("Do you know the latitude/longitude?")
        .default(false)
        .interact()
        .context("failed to read coordinate choice")?;

    let (latitude, longitude) = if enter_coords {
        let lat: f64 = Input::new()
            .with_prompt("Latitude")
            .interact_text()
            .context("failed to read latitude")?;
        let lon: f64 = Input::new()
            .with_prompt("Longitude")
            .interact_text()
            .context("failed to read longitude")?;
        (lat, lon)
    } else {
        (0.0, 0.0)
    };

    let lot_size: f64 = Input::new()
        .with_prompt("Lot size (acres)")
        .default(0.0)
        .interact_text()
        .context("failed to read lot size")?;

    let climate_zone = suggest_climate_zone(&state);
    println!();
    let climate_zone_input: String = Input::new()
        .with_prompt("Climate zone")
        .default(climate_zone.unwrap_or_default())
        .interact_text()
        .context("failed to read climate zone")?;
    let climate_zone = if climate_zone_input.is_empty() {
        None
    } else {
        Some(climate_zone_input)
    };

    let soil_types = ["Clay", "Loam", "Sand", "Silt", "Unknown"];
    let soil_idx = Select::new()
        .with_prompt("Soil type")
        .items(&soil_types)
        .default(4) // Unknown
        .interact()
        .context("failed to read soil type")?;
    let soil_type: SoilType = soil_types[soil_idx]
        .parse()
        .unwrap_or(SoilType::Unknown);

    let mut site = Site::new(address, city, state, zip);
    site.latitude = latitude;
    site.longitude = longitude;
    site.lot_size = Acres::new(lot_size);
    site.climate_zone = climate_zone;
    site.soil_type = Some(soil_type);

    // -----------------------------------------------------------------------
    // 2. Structure setup
    // -----------------------------------------------------------------------
    println!();
    println!("--- Structure Setup ---");
    println!();

    let structure_name: String = Input::new()
        .with_prompt("Structure name")
        .default("House".to_string())
        .interact_text()
        .context("failed to read structure name")?;

    let year_built: String = Input::new()
        .with_prompt("Year built (or blank to skip)")
        .default(String::new())
        .interact_text()
        .context("failed to read year built")?;
    let year_built: Option<i32> = if year_built.is_empty() {
        None
    } else {
        Some(year_built.parse().context("invalid year")?)
    };

    let sqft: f64 = Input::new()
        .with_prompt("Square footage")
        .default(0.0)
        .interact_text()
        .context("failed to read square footage")?;

    let stories: String = Input::new()
        .with_prompt("Number of stories (or blank to skip)")
        .default(String::new())
        .interact_text()
        .context("failed to read stories")?;
    let stories: Option<i32> = if stories.is_empty() {
        None
    } else {
        Some(stories.parse().context("invalid story count")?)
    };

    let foundation_types = ["Slab", "Crawlspace", "Basement", "Pier", "Unknown"];
    let foundation_idx = Select::new()
        .with_prompt("Foundation type")
        .items(&foundation_types)
        .default(4)
        .interact()
        .context("failed to read foundation type")?;
    let foundation_type: FoundationType = foundation_types[foundation_idx]
        .parse()
        .unwrap_or(FoundationType::Unknown);

    let has_pool = Confirm::new()
        .with_prompt("Does the property have a pool?")
        .default(false)
        .interact()
        .context("failed to read pool answer")?;

    let pool_gallons = if has_pool {
        let gal: f64 = Input::new()
            .with_prompt("Pool volume (gallons)")
            .default(0.0)
            .interact_text()
            .context("failed to read pool gallons")?;
        Some(gal)
    } else {
        None
    };

    let has_septic = Confirm::new()
        .with_prompt("Does the property have a septic system?")
        .default(false)
        .interact()
        .context("failed to read septic answer")?;

    let mut structure = Structure::new(site.id, structure_name);
    structure.year_built = year_built;
    structure.square_footage = SquareFeet::new(sqft);
    structure.stories = stories;
    structure.foundation_type = Some(foundation_type);
    structure.has_pool = has_pool;
    structure.pool_gallons = pool_gallons;
    structure.has_septic = has_septic;

    // -----------------------------------------------------------------------
    // 3. Zone setup
    // -----------------------------------------------------------------------
    println!();
    println!("--- Zone / Room Setup ---");
    println!("Add rooms and zones. Type \"done\" for the name when finished.");
    println!();

    let mut zones: Vec<Zone> = Vec::new();
    loop {
        let zone_name: String = Input::new()
            .with_prompt("Zone/room name (or \"done\")")
            .interact_text()
            .context("failed to read zone name")?;

        if zone_name.trim().eq_ignore_ascii_case("done") {
            break;
        }

        let floor: String = Input::new()
            .with_prompt("Floor number (or blank)")
            .default(String::new())
            .interact_text()
            .context("failed to read floor")?;
        let floor: Option<i32> = if floor.is_empty() {
            None
        } else {
            Some(floor.parse().context("invalid floor number")?)
        };

        let mut zone = Zone::new(structure.id, zone_name);
        zone.floor = floor;
        zones.push(zone);
    }

    // -----------------------------------------------------------------------
    // 4. Utility account setup
    // -----------------------------------------------------------------------
    println!();
    println!("--- Utility Accounts ---");
    println!("Add your utility providers. Type \"done\" for the provider when finished.");
    println!();

    let utility_type_labels = [
        "Electric", "Gas", "Water", "Sewer", "Trash", "Internet", "Propane",
    ];
    let utility_type_values = [
        UtilityType::Electric,
        UtilityType::Gas,
        UtilityType::Water,
        UtilityType::Sewer,
        UtilityType::Trash,
        UtilityType::Internet,
        UtilityType::Propane,
    ];

    let mut accounts: Vec<UtilityAccount> = Vec::new();
    loop {
        let provider: String = Input::new()
            .with_prompt("Provider name (or \"done\")")
            .interact_text()
            .context("failed to read provider name")?;

        if provider.trim().eq_ignore_ascii_case("done") {
            break;
        }

        let type_idx = Select::new()
            .with_prompt("Utility type")
            .items(&utility_type_labels)
            .default(0)
            .interact()
            .context("failed to read utility type")?;

        let acct_number: String = Input::new()
            .with_prompt("Account number (or blank to skip)")
            .default(String::new())
            .interact_text()
            .context("failed to read account number")?;

        let mut account =
            UtilityAccount::new(site.id, provider, utility_type_values[type_idx]);
        if !acct_number.is_empty() {
            account.account_number = Some(acct_number);
        }
        accounts.push(account);
    }

    // -----------------------------------------------------------------------
    // 5. Summary
    // -----------------------------------------------------------------------
    println!();
    println!("==========================================================");
    println!("  Setup Summary");
    println!("==========================================================");
    println!();
    println!("  Site");
    println!("    Address:      {}, {}, {} {}", site.address, site.city, site.state, site.zip);
    if site.latitude != 0.0 || site.longitude != 0.0 {
        println!("    Coordinates:  {:.6}, {:.6}", site.latitude, site.longitude);
    }
    println!("    Lot size:     {}", site.lot_size);
    if let Some(ref cz) = site.climate_zone {
        println!("    Climate zone: {}", cz);
    }
    if let Some(ref st) = site.soil_type {
        println!("    Soil type:    {}", st);
    }
    println!();
    println!("  Structure: {}", structure.name);
    if let Some(yb) = structure.year_built {
        println!("    Year built:   {}", yb);
    }
    println!("    Sq footage:   {}", structure.square_footage);
    if let Some(s) = structure.stories {
        println!("    Stories:      {}", s);
    }
    if let Some(ref ft) = structure.foundation_type {
        println!("    Foundation:   {}", ft);
    }
    if structure.has_pool {
        println!(
            "    Pool:         yes ({})",
            structure
                .pool_gallons
                .map(|g| format!("{:.0} gal", g))
                .unwrap_or_else(|| "unknown size".into())
        );
    }
    if structure.has_septic {
        println!("    Septic:       yes");
    }
    println!();

    if !zones.is_empty() {
        println!("  Zones ({}):", zones.len());
        for z in &zones {
            let floor_str = z.floor.map(|f| format!(" (floor {})", f)).unwrap_or_default();
            println!("    - {}{}", z.name, floor_str);
        }
        println!();
    }

    if !accounts.is_empty() {
        println!("  Utility Accounts ({}):", accounts.len());
        for a in &accounts {
            let acct_str = a
                .account_number
                .as_deref()
                .map(|n| format!(" [{}]", n))
                .unwrap_or_default();
            println!("    - {} ({}){}", a.provider_name, a.utility_type, acct_str);
        }
        println!();
    }

    // -----------------------------------------------------------------------
    // 6. Confirm and save
    // -----------------------------------------------------------------------
    let confirmed = Confirm::new()
        .with_prompt("Save this configuration?")
        .default(true)
        .interact()
        .context("failed to read confirmation")?;

    if !confirmed {
        println!("Aborted. Nothing was saved.");
        return Ok(());
    }

    // Persist everything
    lothal_db::site::insert_site(pool, &site).await?;
    lothal_db::site::insert_structure(pool, &structure).await?;

    for zone in &zones {
        lothal_db::site::insert_zone(pool, zone).await?;
    }

    for account in &accounts {
        lothal_db::bill::insert_utility_account(pool, account).await?;
    }

    println!();
    println!("Site created successfully! (id: {})", site.id);
    println!();
    println!("Next steps:");
    println!("  lothal device add     -- register devices (HVAC, appliances, etc.)");
    println!("  lothal bill add       -- enter utility bills");
    println!("  lothal bill import    -- import bills from PDF");
    println!("  lothal site show      -- view your full ontology tree");
    println!();

    Ok(())
}

/// Suggest an IECC climate zone based on state abbreviation.
fn suggest_climate_zone(state: &str) -> Option<String> {
    match state.to_uppercase().as_str() {
        // Zone 1 - Very Hot Humid
        "HI" => Some("1A - Very Hot Humid".into()),
        "GU" | "VI" | "PR" => Some("1A - Very Hot Humid".into()),
        // Zone 2 - Hot
        "FL" => Some("2A - Hot Humid".into()),
        "TX" => Some("2A - Hot Humid".into()),
        "LA" => Some("2A - Hot Humid".into()),
        "MS" => Some("2A - Hot Humid".into()),
        "AZ" => Some("2B - Hot Dry".into()),
        // Zone 3 - Warm
        "GA" | "SC" | "AL" => Some("3A - Warm Humid".into()),
        "AR" => Some("3A - Warm Humid".into()),
        "NC" | "TN" => Some("3A - Warm Humid".into()),
        "NM" => Some("3B - Warm Dry".into()),
        "CA" => Some("3B - Warm Dry".into()),
        "NV" => Some("3B - Warm Dry".into()),
        // Zone 4 - Mixed
        "OK" => Some("3A - Warm Humid".into()),
        "VA" | "KY" | "WV" | "MO" | "KS" => Some("4A - Mixed Humid".into()),
        "MD" | "DE" | "DC" | "NJ" => Some("4A - Mixed Humid".into()),
        "OR" => Some("4C - Mixed Marine".into()),
        "WA" => Some("4C - Mixed Marine".into()),
        // Zone 5 - Cool
        "OH" | "IN" | "IL" | "PA" | "NY" | "CT" | "RI" | "MA" => {
            Some("5A - Cool Humid".into())
        }
        "IA" | "NE" | "SD" => Some("5A - Cool Humid".into()),
        "CO" | "UT" | "ID" => Some("5B - Cool Dry".into()),
        // Zone 6 - Cold
        "MI" | "WI" | "VT" | "NH" | "ME" => Some("6A - Cold Humid".into()),
        "MT" | "WY" | "ND" => Some("6B - Cold Dry".into()),
        "MN" => Some("6A - Cold Humid".into()),
        // Zone 7 - Very Cold
        "AK" => Some("7 - Very Cold".into()),
        _ => None,
    }
}
