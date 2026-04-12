//! Demo seeder: executes the minimal bootstrap fixture at
//! `migrations/seed/001_bootstrap.sql` so a fresh install has enough ontology
//! (site, structure, utility accounts, panel, circuits) for the dashboard to
//! render without fabricating bills or briefings.
//!
//! The fixture is a plain SQL file — edit it directly rather than adding
//! helper functions here. No readings, no bills, no briefings: those must
//! come from real ingest, not fiction.
//!
//! The file is embedded at compile time via `include_str!` so `lothal
//! demo-seed` works when the binary is run from outside the source tree.

use anyhow::{Context, Result};
use sqlx::PgPool;

const BOOTSTRAP_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../migrations/seed/001_bootstrap.sql"
));

pub async fn seed(pool: &PgPool) -> Result<()> {
    // Guard: refuse to run if a site already exists at the demo address.
    // Avoids stomping over a partially-populated install.
    let existing = lothal_db::site::list_sites(pool).await?;
    if let Some(site) = existing
        .iter()
        .find(|s| s.address == "2451 N Division St")
    {
        anyhow::bail!(
            "A site already exists at {} ({}). Refusing to re-seed — drop the database or use `lothal site edit` instead.",
            site.address,
            site.city,
        );
    }

    println!("Seeding Guthrie bootstrap fixture from migrations/seed/001_bootstrap.sql...");

    // Execute the entire fixture in a single transaction. The SQL file wraps
    // its own BEGIN/COMMIT, so we issue it as one multi-statement payload.
    sqlx::raw_sql(BOOTSTRAP_SQL)
        .execute(pool)
        .await
        .context("executing bootstrap fixture")?;

    // Summary row counts — proves what landed without re-describing the SQL.
    let site_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sites")
        .fetch_one(pool)
        .await?;
    let structure_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM structures")
        .fetch_one(pool)
        .await?;
    let account_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM utility_accounts")
        .fetch_one(pool)
        .await?;
    let panel_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM panels")
        .fetch_one(pool)
        .await?;
    let circuit_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM circuits")
        .fetch_one(pool)
        .await?;

    println!();
    println!("Seeded:");
    println!("  sites:            {site_count}");
    println!("  structures:       {structure_count}");
    println!("  utility_accounts: {account_count}");
    println!("  panels:           {panel_count}");
    println!("  circuits:         {circuit_count}");
    println!();
    println!("No bills, readings, or briefings were seeded — those come from real ingest.");
    println!("Next steps:");
    println!("  lothal ingest weather   # pull NWS history");
    println!("  lothal bill import <pdf>  # or let the daemon poll email");

    Ok(())
}
