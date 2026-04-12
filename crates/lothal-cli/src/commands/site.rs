use anyhow::{bail, Context, Result};
use comfy_table::{Cell, Table};
use dialoguer::{Confirm, Input, Select};
use sqlx::PgPool;

use lothal_core::ontology::site::SoilType;
use lothal_core::units::Acres;

/// Display the full ontology tree for the first (or only) site.
///
/// Shows site info, structures with their zones, devices, and circuits
/// in a hierarchical tree format.
pub async fn show_site(pool: &PgPool) -> Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = match sites.first() {
        Some(s) => s,
        None => {
            println!("No site found. Run `lothal init` to set up your property.");
            return Ok(());
        }
    };

    // Header
    println!();
    println!("=== {} ===", site.address);
    println!("    {}, {} {}", site.city, site.state, site.zip);
    if site.latitude != 0.0 || site.longitude != 0.0 {
        println!("    Coordinates: {:.6}, {:.6}", site.latitude, site.longitude);
    }
    println!("    Lot size:     {}", site.lot_size);
    if let Some(ref cz) = site.climate_zone {
        println!("    Climate zone: {}", cz);
    }
    if let Some(ref st) = site.soil_type {
        println!("    Soil type:    {}", st);
    }
    println!("    ID:           {}", site.id);
    println!();

    // Structures
    let structures = lothal_db::site::get_structures_by_site(pool, site.id).await?;
    if structures.is_empty() {
        println!("    (no structures)");
        return Ok(());
    }

    for structure in &structures {
        println!(
            "  {} {}",
            "\u{250c}\u{2500}\u{2500}", // box-drawing: top-left corner
            structure.name
        );

        // Structure details
        let mut details = Vec::new();
        if let Some(yb) = structure.year_built {
            details.push(format!("built {}", yb));
        }
        if structure.square_footage.value() > 0.0 {
            details.push(format!("{}", structure.square_footage));
        }
        if let Some(s) = structure.stories {
            details.push(format!("{} stories", s));
        }
        if let Some(ref ft) = structure.foundation_type {
            details.push(format!("{} foundation", ft));
        }
        if structure.has_pool {
            let pool_str = structure
                .pool_gallons
                .map(|g| format!("pool ({:.0} gal)", g))
                .unwrap_or_else(|| "pool".into());
            details.push(pool_str);
        }
        if structure.has_septic {
            details.push("septic".into());
        }
        if !details.is_empty() {
            println!(
                "  {}   {}",
                "\u{2502}", // vertical bar
                details.join(" | ")
            );
        }

        // Zones
        let zones = lothal_db::site::get_zones_by_structure(pool, structure.id).await?;
        if !zones.is_empty() {
            println!("  {}   Zones:", "\u{2502}");
            for (i, zone) in zones.iter().enumerate() {
                let connector = if i == zones.len() - 1 {
                    "\u{2514}\u{2500}" // bottom-left corner
                } else {
                    "\u{251c}\u{2500}" // tee
                };
                let floor_str = zone
                    .floor
                    .map(|f| format!(" (floor {})", f))
                    .unwrap_or_default();
                println!("  {}     {} {}{}", "\u{2502}", connector, zone.name, floor_str);
            }
        }

        // Devices
        let devices =
            lothal_db::device::list_devices_by_structure(pool, structure.id).await?;
        if !devices.is_empty() {
            println!("  {}   Devices:", "\u{2502}");
            let mut table = Table::new();
            table.set_header(vec!["Name", "Kind", "Watts", "Daily Hours", "Est. kWh/yr"]);
            for device in &devices {
                let watts_str = device
                    .nameplate_watts
                    .map(|w| format!("{:.0}", w.value()))
                    .unwrap_or_else(|| "-".into());
                let hours_str = device
                    .estimated_daily_hours
                    .map(|h| format!("{:.1}", h))
                    .unwrap_or_else(|| "-".into());
                let kwh_str = device
                    .estimated_annual_kwh()
                    .map(|k| format!("{:.0}", k))
                    .unwrap_or_else(|| "-".into());
                table.add_row(vec![
                    Cell::new(&device.name),
                    Cell::new(device.kind.to_string()),
                    Cell::new(&watts_str),
                    Cell::new(&hours_str),
                    Cell::new(&kwh_str),
                ]);
            }
            // Indent the table output
            for line in table.to_string().lines() {
                println!("  {}     {}", "\u{2502}", line);
            }
        }

        // Panels / circuits
        let panels =
            lothal_db::site::get_panels_by_structure(pool, structure.id).await?;
        if !panels.is_empty() {
            println!("  {}   Panels:", "\u{2502}");
            for panel in &panels {
                let amp_str = panel
                    .amperage
                    .map(|a| format!(" ({}A)", a))
                    .unwrap_or_default();
                let main_str = if panel.is_main { " [main]" } else { "" };
                println!(
                    "  {}     {}{}{}", "\u{2502}", panel.name, amp_str, main_str
                );

                let circuits =
                    lothal_db::device::get_circuits_by_panel(pool, panel.id).await?;
                for circuit in &circuits {
                    let pole_str = if circuit.is_double_pole {
                        " (double)"
                    } else {
                        ""
                    };
                    println!(
                        "  {}       #{:>2} {:>3}A {} {}",
                        "\u{2502}",
                        circuit.breaker_number,
                        circuit.amperage,
                        circuit.label,
                        pole_str,
                    );
                }
            }
        }

        println!("  {}", "\u{2514}\u{2500}\u{2500}");
    }

    // Utility accounts summary
    let accounts =
        lothal_db::bill::list_utility_accounts_by_site(pool, site.id).await?;
    if !accounts.is_empty() {
        println!();
        println!("  Utility Accounts:");
        let mut table = Table::new();
        table.set_header(vec!["Provider", "Type", "Account #", "Active"]);
        for acct in &accounts {
            table.add_row(vec![
                Cell::new(&acct.provider_name),
                Cell::new(acct.utility_type.to_string()),
                Cell::new(
                    acct.account_number
                        .as_deref()
                        .unwrap_or("-"),
                ),
                Cell::new(if acct.is_active { "yes" } else { "no" }),
            ]);
        }
        for line in table.to_string().lines() {
            println!("    {}", line);
        }
    }

    println!();
    Ok(())
}

/// Interactive editing of site properties.
pub async fn edit_site(pool: &PgPool) -> Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let mut site = match sites.into_iter().next() {
        Some(s) => s,
        None => {
            bail!("No site found. Run `lothal init` first.");
        }
    };

    println!();
    println!("Editing site: {}", site.address);
    println!("Press Enter to keep the current value.");
    println!();

    let address: String = Input::new()
        .with_prompt("Address")
        .default(site.address.clone())
        .interact_text()
        .context("failed to read address")?;
    site.address = address;

    let city: String = Input::new()
        .with_prompt("City")
        .default(site.city.clone())
        .interact_text()
        .context("failed to read city")?;
    site.city = city;

    let state: String = Input::new()
        .with_prompt("State")
        .default(site.state.clone())
        .interact_text()
        .context("failed to read state")?;
    site.state = state;

    let zip: String = Input::new()
        .with_prompt("ZIP")
        .default(site.zip.clone())
        .interact_text()
        .context("failed to read zip")?;
    site.zip = zip;

    let latitude: f64 = Input::new()
        .with_prompt("Latitude")
        .default(site.latitude)
        .interact_text()
        .context("failed to read latitude")?;
    site.latitude = latitude;

    let longitude: f64 = Input::new()
        .with_prompt("Longitude")
        .default(site.longitude)
        .interact_text()
        .context("failed to read longitude")?;
    site.longitude = longitude;

    let lot_size: f64 = Input::new()
        .with_prompt("Lot size (acres)")
        .default(site.lot_size.value())
        .interact_text()
        .context("failed to read lot size")?;
    site.lot_size = Acres::new(lot_size);

    let cz_default = site.climate_zone.clone().unwrap_or_default();
    let climate_zone: String = Input::new()
        .with_prompt("Climate zone")
        .default(cz_default)
        .interact_text()
        .context("failed to read climate zone")?;
    site.climate_zone = if climate_zone.is_empty() {
        None
    } else {
        Some(climate_zone)
    };

    let soil_types = ["Clay", "Loam", "Sand", "Silt", "Unknown"];
    let current_soil_idx = site
        .soil_type
        .map(|st| match st {
            SoilType::Clay => 0,
            SoilType::Loam => 1,
            SoilType::Sand => 2,
            SoilType::Silt => 3,
            SoilType::Unknown => 4,
        })
        .unwrap_or(4);
    let soil_idx = Select::new()
        .with_prompt("Soil type")
        .items(&soil_types)
        .default(current_soil_idx)
        .interact()
        .context("failed to read soil type")?;
    site.soil_type = Some(
        soil_types[soil_idx]
            .parse()
            .unwrap_or(SoilType::Unknown),
    );

    site.updated_at = chrono::Utc::now();

    let confirmed = Confirm::new()
        .with_prompt("Save changes?")
        .default(true)
        .interact()
        .context("failed to read confirmation")?;

    if !confirmed {
        println!("Cancelled.");
        return Ok(());
    }

    lothal_db::site::update_site(pool, &site).await?;
    println!("Site updated.");
    Ok(())
}
