//! Demo seeder: populate a Guthrie-shaped fixture so the dashboard shows
//! meaningful content on day one before real bills and readings arrive.

use anyhow::Result;
use chrono::{Datelike, NaiveDate, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use lothal_core::ontology::bill::Bill;
use lothal_core::ontology::site::{FoundationType, Site, SoilType, Structure};
use lothal_core::ontology::utility::{UtilityAccount, UtilityType};
use lothal_core::ontology::water::{CoverType, Pool, SepticSystem};
use lothal_core::units::{Acres, Gallons, SquareFeet, Usd};

pub async fn seed(pool: &PgPool) -> Result<()> {
    let existing = lothal_db::site::list_sites(pool).await?;
    if !existing.is_empty() {
        anyhow::bail!(
            "A site already exists ({}). Refusing to overwrite — drop the database or use `lothal site edit` instead.",
            existing[0].address,
        );
    }

    println!("Seeding demo data for a Guthrie, OK property...");

    // ----- Site -----
    let mut site = Site::new(
        "2451 N Division St".into(),
        "Guthrie".into(),
        "OK".into(),
        "73044".into(),
    );
    site.latitude = 35.8986;
    site.longitude = -97.4254;
    site.lot_size = Acres::new(2.5);
    site.climate_zone = Some("3A - Warm Humid".into());
    site.soil_type = Some(SoilType::Clay);
    lothal_db::site::insert_site(pool, &site).await?;
    println!("  Site: {}, {}", site.address, site.city);

    // ----- Structure -----
    let mut structure = Structure::new(site.id, "Main House".into());
    structure.year_built = Some(1998);
    structure.square_footage = SquareFeet::new(2400.0);
    structure.stories = Some(1);
    structure.foundation_type = Some(FoundationType::Slab);
    structure.has_pool = true;
    structure.pool_gallons = Some(18000.0);
    structure.has_septic = true;
    lothal_db::site::insert_structure(pool, &structure).await?;
    println!(
        "  Structure: {} ({} sqft, {} built)",
        structure.name,
        structure.square_footage,
        structure.year_built.unwrap_or(0)
    );

    // ----- Utility accounts -----
    let electric =
        UtilityAccount::new(site.id, "OG&E".into(), UtilityType::Electric);
    let gas = UtilityAccount::new(site.id, "ONG".into(), UtilityType::Gas);
    let water = UtilityAccount::new(site.id, "City of Guthrie".into(), UtilityType::Water);
    for a in [&electric, &gas, &water] {
        lothal_db::bill::insert_utility_account(pool, a).await?;
    }
    println!("  Utility accounts: OG&E, ONG, City of Guthrie");

    // ----- Bills (6 months of electric, 3 of gas, 2 of water) -----
    let today = Utc::now().date_naive();
    let mut bill_count = 0;

    // Electric: seasonal curve — summer peaks at ~1200 kWh, shoulder ~700, winter ~900
    let electric_months = [
        (6, 1240.0, 136.40), // June
        (5, 880.0, 96.80),   // May
        (4, 640.0, 70.40),   // April
        (3, 720.0, 79.20),   // March
        (2, 920.0, 101.20),  // February
        (1, 1020.0, 112.20), // January
    ];
    for (months_ago, kwh, cost) in electric_months {
        let bill = build_monthly_bill(
            electric.id,
            today,
            months_ago,
            kwh,
            "kWh".into(),
            Usd::new(cost),
        );
        lothal_db::bill::insert_bill(pool, &bill).await?;
        bill_count += 1;
    }

    // Gas: winter peak
    let gas_months = [
        (3, 12.0, 32.40),  // March (shoulder)
        (2, 62.0, 96.80),  // February (peak)
        (1, 48.0, 78.20),  // January
    ];
    for (months_ago, therms, cost) in gas_months {
        let bill = build_monthly_bill(
            gas.id,
            today,
            months_ago,
            therms,
            "therms".into(),
            Usd::new(cost),
        );
        lothal_db::bill::insert_bill(pool, &bill).await?;
        bill_count += 1;
    }

    // Water: low baseline + small irrigation bump
    let water_months = [(2, 4200.0, 42.80), (1, 5100.0, 51.20)];
    for (months_ago, gallons, cost) in water_months {
        let bill = build_monthly_bill(
            water.id,
            today,
            months_ago,
            gallons,
            "gallons".into(),
            Usd::new(cost),
        );
        lothal_db::bill::insert_bill(pool, &bill).await?;
        bill_count += 1;
    }
    println!("  Bills: {bill_count} historical statements");

    // ----- Pool -----
    let mut pool_entity = Pool::new(site.id, "Backyard Pool".into(), Gallons::new(18000.0));
    pool_entity.surface_area_sqft = Some(SquareFeet::new(450.0));
    pool_entity.cover_type = Some(CoverType::Manual);
    lothal_db::water::insert_pool(pool, &pool_entity).await?;
    println!("  Pool: {} ({} gal)", pool_entity.name, pool_entity.volume_gallons);

    // ----- Septic -----
    let mut septic = SepticSystem::new(site.id);
    septic.tank_capacity_gallons = Some(Gallons::new(1000.0));
    septic.pump_interval_months = Some(36);
    septic.last_pumped = today.checked_sub_months(chrono::Months::new(33));
    septic.daily_load_estimate_gallons = Some(180.0);
    lothal_db::water::insert_septic_system(pool, &septic).await?;
    println!("  Septic: 1000 gal tank, due in ~3 months");

    // ----- Seed a sample briefing so Pulse renders something today -----
    let yesterday = today - chrono::Duration::days(1);
    let briefing = "Yesterday used 28.9 kWh ($3.18), 6% below the weather-normalized baseline of 30.7 kWh on a mild 68°F day (CDD 3). Pool pump ran 6.2h (normal). Septic pump-out is due in 94 days — worth scheduling ahead of summer bookings. No circuit anomalies detected.";
    sqlx::query(
        r#"INSERT INTO briefings (id, site_id, date, content, context, model, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, now())
           ON CONFLICT (site_id, date) DO NOTHING"#,
    )
    .bind(Uuid::new_v4())
    .bind(site.id)
    .bind(yesterday)
    .bind(briefing)
    .bind(serde_json::Value::Null)
    .bind("demo-seed")
    .execute(pool)
    .await?;
    println!("  Seeded briefing for {yesterday}");

    println!();
    println!("Demo data seeded. Start the web dashboard with:");
    println!("  cargo run -p lothal-web");
    println!("Then browse to http://localhost:3000");

    Ok(())
}

fn build_monthly_bill(
    account_id: Uuid,
    today: NaiveDate,
    months_ago: u32,
    usage: f64,
    unit: String,
    total: Usd,
) -> Bill {
    let statement_month = today
        .with_day(1)
        .unwrap()
        .checked_sub_months(chrono::Months::new(months_ago))
        .unwrap_or(today);
    let period_start = statement_month;
    let period_end = period_start
        .checked_add_months(chrono::Months::new(1))
        .unwrap_or(period_start)
        - chrono::Duration::days(1);
    let statement_date = period_end + chrono::Duration::days(3);
    Bill::new(account_id, period_start, period_end, statement_date, usage, unit, total)
}
