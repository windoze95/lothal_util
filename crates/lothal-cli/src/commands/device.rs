use anyhow::{bail, Context, Result};
use comfy_table::{Cell, Table};
use dialoguer::{Confirm, Input, Select};
use sqlx::PgPool;
use uuid::Uuid;

use lothal_core::ontology::device::{Device, DeviceKind};
use lothal_core::units::Watts;

/// All device kinds in display order, matching the DeviceKind enum variants.
const DEVICE_KIND_LABELS: &[&str] = &[
    // HVAC
    "Air Conditioner",
    "Furnace",
    "Heat Pump",
    "Air Handler",
    "Thermostat",
    // Water
    "Water Heater",
    "Water Softener",
    "Well Pump",
    // Pool
    "Pool Pump",
    "Pool Heater",
    "Pool Cleaner",
    // Kitchen
    "Refrigerator",
    "Freezer",
    "Dishwasher",
    "Oven",
    "Range",
    "Microwave",
    // Laundry
    "Washer",
    "Dryer",
    // Comfort
    "Dehumidifier",
    "Humidifier",
    "Ceiling Fan",
    "Space Heater",
    // Infrastructure
    "Electrical Panel",
    "Sump Pump",
    "Garage Door",
    "Security System",
    // Tech
    "Server",
    "Network Switch",
    "UPS",
    // Outdoor
    "Sprinkler",
    "Outdoor Lighting",
    "EV Charger",
    // Catch-all
    "Other",
];

const DEVICE_KIND_VALUES: &[DeviceKind] = &[
    DeviceKind::AirConditioner,
    DeviceKind::Furnace,
    DeviceKind::HeatPump,
    DeviceKind::AirHandler,
    DeviceKind::Thermostat,
    DeviceKind::WaterHeater,
    DeviceKind::WaterSoftener,
    DeviceKind::WellPump,
    DeviceKind::PoolPump,
    DeviceKind::PoolHeater,
    DeviceKind::PoolCleaner,
    DeviceKind::Refrigerator,
    DeviceKind::Freezer,
    DeviceKind::Dishwasher,
    DeviceKind::Oven,
    DeviceKind::Range,
    DeviceKind::Microwave,
    DeviceKind::Washer,
    DeviceKind::Dryer,
    DeviceKind::Dehumidifier,
    DeviceKind::Humidifier,
    DeviceKind::CeilingFan,
    DeviceKind::SpaceHeater,
    DeviceKind::ElectricalPanel,
    DeviceKind::SumpPump,
    DeviceKind::GarageDoor,
    DeviceKind::SecuritySystem,
    DeviceKind::Server,
    DeviceKind::NetworkSwitch,
    DeviceKind::UPS,
    DeviceKind::Sprinkler,
    DeviceKind::OutdoorLighting,
    DeviceKind::EvCharger,
    DeviceKind::Other,
];

/// Interactive device registration wizard.
pub async fn add_device(pool: &PgPool) -> Result<()> {
    // Pick a structure
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites
        .first()
        .ok_or_else(|| anyhow::anyhow!("No site found. Run `lothal init` first."))?;

    let structures = lothal_db::site::get_structures_by_site(pool, site.id).await?;
    if structures.is_empty() {
        bail!("No structures found. Run `lothal init` first.");
    }

    let structure = if structures.len() == 1 {
        &structures[0]
    } else {
        let labels: Vec<&str> = structures.iter().map(|s| s.name.as_str()).collect();
        let idx = Select::new()
            .with_prompt("Select structure")
            .items(&labels)
            .default(0)
            .interact()
            .context("failed to select structure")?;
        &structures[idx]
    };

    println!();
    println!("Adding device to: {}", structure.name);
    println!();

    let name: String = Input::new()
        .with_prompt("Device name")
        .interact_text()
        .context("failed to read device name")?;

    let kind_idx = Select::new()
        .with_prompt("Device type")
        .items(DEVICE_KIND_LABELS)
        .default(0)
        .interact()
        .context("failed to read device type")?;
    let kind = DEVICE_KIND_VALUES[kind_idx];

    let watts_str: String = Input::new()
        .with_prompt("Nameplate watts (or blank)")
        .default(String::new())
        .interact_text()
        .context("failed to read watts")?;
    let nameplate_watts: Option<Watts> = if watts_str.is_empty() {
        None
    } else {
        Some(Watts::new(watts_str.parse().context("invalid wattage")?))
    };

    let hours_str: String = Input::new()
        .with_prompt("Estimated daily run hours (or blank)")
        .default(String::new())
        .interact_text()
        .context("failed to read hours")?;
    let estimated_daily_hours: Option<f64> = if hours_str.is_empty() {
        None
    } else {
        Some(hours_str.parse().context("invalid hours")?)
    };

    let year_str: String = Input::new()
        .with_prompt("Year installed (or blank)")
        .default(String::new())
        .interact_text()
        .context("failed to read year")?;
    let year_installed: Option<i32> = if year_str.is_empty() {
        None
    } else {
        Some(year_str.parse().context("invalid year")?)
    };

    let make: String = Input::new()
        .with_prompt("Make/manufacturer (or blank)")
        .default(String::new())
        .interact_text()
        .context("failed to read make")?;

    let model: String = Input::new()
        .with_prompt("Model (or blank)")
        .default(String::new())
        .interact_text()
        .context("failed to read model")?;

    // Optionally assign to a zone
    let zones = lothal_db::site::get_zones_by_structure(pool, structure.id).await?;
    let zone_id = if !zones.is_empty() {
        let assign_zone = Confirm::new()
            .with_prompt("Assign to a zone?")
            .default(false)
            .interact()
            .context("failed to read zone choice")?;
        if assign_zone {
            let zone_labels: Vec<&str> = zones.iter().map(|z| z.name.as_str()).collect();
            let idx = Select::new()
                .with_prompt("Select zone")
                .items(&zone_labels)
                .default(0)
                .interact()
                .context("failed to select zone")?;
            Some(zones[idx].id)
        } else {
            None
        }
    } else {
        None
    };

    // Optionally assign to a circuit
    let panels = lothal_db::site::get_panels_by_structure(pool, structure.id).await?;
    let circuit_id = if !panels.is_empty() {
        let assign_circuit = Confirm::new()
            .with_prompt("Assign to a circuit?")
            .default(false)
            .interact()
            .context("failed to read circuit choice")?;
        if assign_circuit {
            // Collect all circuits across panels
            let mut all_circuits = Vec::new();
            let mut circuit_labels = Vec::new();
            for panel in &panels {
                let circuits =
                    lothal_db::device::get_circuits_by_panel(pool, panel.id).await?;
                for circuit in circuits {
                    circuit_labels.push(format!(
                        "{} - #{} {} ({}A)",
                        panel.name, circuit.breaker_number, circuit.label, circuit.amperage
                    ));
                    all_circuits.push(circuit);
                }
            }
            if all_circuits.is_empty() {
                None
            } else {
                let idx = Select::new()
                    .with_prompt("Select circuit")
                    .items(&circuit_labels)
                    .default(0)
                    .interact()
                    .context("failed to select circuit")?;
                Some(all_circuits[idx].id)
            }
        } else {
            None
        }
    } else {
        None
    };

    let mut device = Device::new(structure.id, name, kind);
    device.nameplate_watts = nameplate_watts;
    device.estimated_daily_hours = estimated_daily_hours;
    device.year_installed = year_installed;
    device.make = if make.is_empty() { None } else { Some(make) };
    device.model = if model.is_empty() {
        None
    } else {
        Some(model)
    };
    device.zone_id = zone_id;
    device.circuit_id = circuit_id;

    // Show estimated annual kWh if we can compute it
    if let Some(kwh) = device.estimated_annual_kwh() {
        println!();
        println!("Estimated annual usage: {:.0} kWh", kwh);
    }

    lothal_db::device::insert_device(pool, &device).await?;
    println!();
    println!("Device added: {} (id: {})", device.name, device.id);
    Ok(())
}

/// Display a table of all devices across all structures.
pub async fn list_devices(pool: &PgPool) -> Result<()> {
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites
        .first()
        .ok_or_else(|| anyhow::anyhow!("No site found. Run `lothal init` first."))?;

    let structures = lothal_db::site::get_structures_by_site(pool, site.id).await?;

    let mut all_devices = Vec::new();
    for structure in &structures {
        let devices =
            lothal_db::device::list_devices_by_structure(pool, structure.id).await?;
        all_devices.extend(devices);
    }

    if all_devices.is_empty() {
        println!("No devices registered. Run `lothal device add` to add one.");
        return Ok(());
    }

    let mut table = Table::new();
    table.set_header(vec![
        "ID (short)",
        "Name",
        "Kind",
        "Watts",
        "Daily Hrs",
        "Est. kWh/yr",
    ]);

    for device in &all_devices {
        let short_id = &device.id.to_string()[..8];
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
            Cell::new(short_id),
            Cell::new(&device.name),
            Cell::new(device.kind.to_string()),
            Cell::new(&watts_str),
            Cell::new(&hours_str),
            Cell::new(&kwh_str),
        ]);
    }

    println!();
    println!("{table}");
    println!();
    println!("{} device(s) total", all_devices.len());

    // Show aggregate estimated annual kWh
    let total_kwh: f64 = all_devices
        .iter()
        .filter_map(|d| d.estimated_annual_kwh())
        .sum();
    if total_kwh > 0.0 {
        println!("Estimated total annual usage: {:.0} kWh", total_kwh);
    }

    println!();
    Ok(())
}

/// Show detailed information for a single device.
pub async fn show_device(pool: &PgPool, id: &str) -> Result<()> {
    let uuid = parse_device_id(pool, id).await?;

    let device = lothal_db::device::get_device(pool, uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Device not found: {}", id))?;

    println!();
    println!("=== {} ===", device.name);
    println!("  ID:              {}", device.id);
    println!("  Kind:            {}", device.kind);
    if let Some(ref make) = device.make {
        println!("  Make:            {}", make);
    }
    if let Some(ref model) = device.model {
        println!("  Model:           {}", model);
    }
    if let Some(w) = device.nameplate_watts {
        println!("  Nameplate watts: {:.0} W", w.value());
    }
    if let Some(h) = device.estimated_daily_hours {
        println!("  Daily hours:     {:.1}", h);
    }
    if let Some(kwh) = device.estimated_annual_kwh() {
        println!("  Est. annual kWh: {:.0}", kwh);
    }
    if let Some(y) = device.year_installed {
        println!("  Year installed:  {}", y);
    }
    if let Some(l) = device.expected_lifespan_years {
        println!("  Exp. lifespan:   {} years", l);
    }
    if let Some(c) = device.replacement_cost {
        println!("  Replacement:     ${:.2}", c.value());
    }
    if let Some(ref notes) = device.notes {
        println!("  Notes:           {}", notes);
    }

    // Zone info
    if let Some(zone_id) = device.zone_id {
        println!("  Zone ID:         {}", zone_id);
    }
    // Circuit info
    if let Some(circuit_id) = device.circuit_id {
        println!("  Circuit ID:      {}", circuit_id);
    }

    println!("  Structure ID:    {}", device.structure_id);
    println!("  Created:         {}", device.created_at.format("%Y-%m-%d %H:%M"));
    println!("  Updated:         {}", device.updated_at.format("%Y-%m-%d %H:%M"));
    println!();

    Ok(())
}

/// Parse a device ID from user input. Supports full UUIDs or short prefixes
/// (at least 8 hex chars). For short prefixes, does a linear scan.
async fn parse_device_id(pool: &PgPool, id: &str) -> Result<Uuid> {
    // Try full UUID first
    if let Ok(uuid) = id.parse::<Uuid>() {
        return Ok(uuid);
    }

    // Short prefix search: find all devices and match prefix
    let sites = lothal_db::site::list_sites(pool).await?;
    let site = sites
        .first()
        .ok_or_else(|| anyhow::anyhow!("No site found"))?;

    let structures = lothal_db::site::get_structures_by_site(pool, site.id).await?;
    let lower_id = id.to_lowercase();

    for structure in &structures {
        let devices =
            lothal_db::device::list_devices_by_structure(pool, structure.id).await?;
        let matches: Vec<_> = devices
            .into_iter()
            .filter(|d| d.id.to_string().starts_with(&lower_id))
            .collect();

        match matches.len() {
            0 => continue,
            1 => return Ok(matches[0].id),
            n => bail!(
                "Ambiguous device ID prefix '{}': matched {} devices. Use more characters.",
                id,
                n
            ),
        }
    }

    bail!("No device found matching '{}'", id)
}
