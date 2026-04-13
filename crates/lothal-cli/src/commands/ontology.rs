//! `lothal ontology` CLI subcommands.
//!
//! The only current subcommand is `backfill`, which walks every domain table
//! whose type has a `Describe` impl and writes its corresponding `objects`
//! and parent `links` rows. This addresses pre-existing rows that were
//! inserted before the repo hooks landed.
//!
//! Backfill is idempotent: `upsert_object` and `upsert_link` both ON CONFLICT,
//! so running this twice is safe. Events are deliberately **not** emitted
//! during backfill — we don't want to flood the hypertable with
//! `*_registered` events for rows that were registered long ago.

use std::collections::BTreeMap;

use anyhow::Result;
use clap::Subcommand;
use sqlx::PgPool;

use lothal_core::ontology::bill::Bill;
use lothal_core::ontology::device::Device;
use lothal_core::ontology::experiment::Experiment;
use lothal_core::ontology::garden::GardenBed;
use lothal_core::ontology::livestock::Flock;
use lothal_core::ontology::maintenance::MaintenanceEvent;
use lothal_core::ontology::property_zone::PropertyZone;
use lothal_core::ontology::site::{Site, Structure};
use lothal_core::ontology::utility::UtilityAccount;
use lothal_core::ontology::water::Pool;
use lothal_db::repo::{bill, device, experiment, garden, livestock, maintenance, property_zone, site, water};
use lothal_ontology::indexer;
use lothal_ontology::{Describe, LinkSpec, ObjectRef};

#[derive(Subcommand, Debug)]
pub enum OntologyCommands {
    /// Backfill the objects, links, and events tables from existing domain rows.
    Backfill {
        /// Dry run — compute counts but roll back instead of committing.
        #[arg(long)]
        dry_run: bool,
    },
}

pub async fn run(pool: &PgPool, cmd: OntologyCommands) -> Result<()> {
    match cmd {
        OntologyCommands::Backfill { dry_run } => backfill(pool, dry_run).await,
    }
}

async fn backfill(pool: &PgPool, dry_run: bool) -> Result<()> {
    let mut counts: BTreeMap<&'static str, u64> = BTreeMap::new();

    // Sites are the root — everything else hangs off them.
    let sites = site::list_sites(pool).await?;
    for site in &sites {
        backfill_one(pool, site, None, dry_run).await?;
        *counts.entry(Site::KIND).or_insert(0) += 1;
    }

    for site in &sites {
        // Structures → Site (contained_in).
        let structures = site::get_structures_by_site(pool, site.id).await?;
        for structure in &structures {
            let link = LinkSpec::new(
                "contained_in",
                ObjectRef::new(Structure::KIND, structure.id),
                ObjectRef::new(Site::KIND, site.id),
            );
            backfill_one(pool, structure, Some(link), dry_run).await?;
            *counts.entry(Structure::KIND).or_insert(0) += 1;

            // Devices → Structure (contained_in).
            let devices = device::list_devices_by_structure(pool, structure.id).await?;
            for d in &devices {
                let link = LinkSpec::new(
                    "contained_in",
                    ObjectRef::new(Device::KIND, d.id),
                    ObjectRef::new("structure", structure.id),
                );
                backfill_one(pool, d, Some(link), dry_run).await?;
                *counts.entry(Device::KIND).or_insert(0) += 1;
            }

            // Circuits → Panels (Panel has no Describe impl, so we skip the
            // panel upsert and only emit `powers` when the circuit is bound
            // to a device — matching the repo hook in device.rs.
            let panels = site::get_panels_by_structure(pool, structure.id).await?;
            for panel in &panels {
                let circuits = device::get_circuits_by_panel(pool, panel.id).await?;
                for c in &circuits {
                    let link = c.device_id.map(|dev_id| {
                        LinkSpec::new(
                            "powers",
                            ObjectRef::new("circuit", c.id),
                            ObjectRef::new("device", dev_id),
                        )
                    });
                    backfill_one(pool, c, link, dry_run).await?;
                    *counts.entry("circuit").or_insert(0) += 1;
                }
            }
        }

        // UtilityAccount → Site (contained_in) + Bills → UtilityAccount (issued_by).
        let accounts = bill::list_utility_accounts_by_site(pool, site.id).await?;
        for account in &accounts {
            let link = LinkSpec::new(
                "contained_in",
                ObjectRef::new(UtilityAccount::KIND, account.id),
                ObjectRef::new(Site::KIND, site.id),
            );
            backfill_one(pool, account, Some(link), dry_run).await?;
            *counts.entry(UtilityAccount::KIND).or_insert(0) += 1;

            let bills = bill::list_bills_by_account(pool, account.id).await?;
            for b in &bills {
                let link = LinkSpec::new(
                    "issued_by",
                    ObjectRef::new(Bill::KIND, b.id),
                    ObjectRef::new(UtilityAccount::KIND, account.id),
                );
                backfill_one(pool, b, Some(link), dry_run).await?;
                *counts.entry(Bill::KIND).or_insert(0) += 1;
            }
        }

        // Flock → Site (contained_in).
        let flocks = livestock::list_flocks_by_site(pool, site.id).await?;
        for flock in &flocks {
            let link = LinkSpec::new(
                "contained_in",
                ObjectRef::new(Flock::KIND, flock.id),
                ObjectRef::new(Site::KIND, site.id),
            );
            backfill_one(pool, flock, Some(link), dry_run).await?;
            *counts.entry(Flock::KIND).or_insert(0) += 1;
        }

        // GardenBed → Site (contained_in).
        let beds = garden::list_garden_beds_by_site(pool, site.id).await?;
        for bed in &beds {
            let link = LinkSpec::new(
                "contained_in",
                ObjectRef::new(GardenBed::KIND, bed.id),
                ObjectRef::new(Site::KIND, site.id),
            );
            backfill_one(pool, bed, Some(link), dry_run).await?;
            *counts.entry(GardenBed::KIND).or_insert(0) += 1;
        }

        // Pool → Site (contained_in).
        let pools = water::list_pools_by_site(pool, site.id).await?;
        for p in &pools {
            let link = LinkSpec::new(
                "contained_in",
                ObjectRef::new(Pool::KIND, p.id),
                ObjectRef::new(Site::KIND, site.id),
            );
            backfill_one(pool, p, Some(link), dry_run).await?;
            *counts.entry(Pool::KIND).or_insert(0) += 1;
        }

        // PropertyZone → Site (contained_in).
        let zones = property_zone::list_property_zones_by_site(pool, site.id).await?;
        for zone in &zones {
            let link = LinkSpec::new(
                "contained_in",
                ObjectRef::new(PropertyZone::KIND, zone.id),
                ObjectRef::new(Site::KIND, site.id),
            );
            backfill_one(pool, zone, Some(link), dry_run).await?;
            *counts.entry(PropertyZone::KIND).or_insert(0) += 1;
        }

        // Experiment → Site (targets).
        let experiments = experiment::list_experiments_by_site(pool, site.id).await?;
        for e in &experiments {
            let link = LinkSpec::new(
                "targets",
                ObjectRef::new(Experiment::KIND, e.id),
                ObjectRef::new(Site::KIND, site.id),
            );
            backfill_one(pool, e, Some(link), dry_run).await?;
            *counts.entry(Experiment::KIND).or_insert(0) += 1;
        }

        // MaintenanceEvent → target (targets). `list_maintenance_by_target`
        // is the only list fn available without inventing new SQL, so walk
        // every target kind that can bear maintenance and union the results.
        let devices = site::get_structures_by_site(pool, site.id)
            .await?
            .into_iter()
            .map(|s| s.id)
            .collect::<Vec<_>>();
        let mut maintenance_targets: Vec<(&'static str, uuid::Uuid)> = Vec::new();
        for structure_id in &devices {
            maintenance_targets.push(("structure", *structure_id));
            for d in device::list_devices_by_structure(pool, *structure_id).await? {
                maintenance_targets.push(("device", d.id));
            }
        }
        for zone in property_zone::list_property_zones_by_site(pool, site.id).await? {
            maintenance_targets.push(("property_zone", zone.id));
        }
        for p in water::list_pools_by_site(pool, site.id).await? {
            maintenance_targets.push(("pool", p.id));
        }

        for (kind, id) in maintenance_targets {
            let events = maintenance::list_maintenance_by_target(pool, kind, id).await?;
            for ev in &events {
                let link = LinkSpec::new(
                    "targets",
                    ObjectRef::new(MaintenanceEvent::KIND, ev.id),
                    ObjectRef::new(ev.target.target_type(), ev.target.target_id()),
                );
                backfill_one(pool, ev, Some(link), dry_run).await?;
                *counts.entry(MaintenanceEvent::KIND).or_insert(0) += 1;
            }
        }
    }

    print_summary(&counts, dry_run);
    Ok(())
}

/// Open a transaction, upsert the object (plus an optional link), and either
/// commit or roll back depending on `dry_run`.
async fn backfill_one<T: Describe>(
    pool: &PgPool,
    obj: &T,
    link: Option<LinkSpec>,
    dry_run: bool,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    indexer::upsert_object(&mut tx, obj).await?;
    if let Some(spec) = link {
        indexer::upsert_link(&mut tx, spec).await?;
    }
    if dry_run {
        tx.rollback().await?;
    } else {
        tx.commit().await?;
    }
    Ok(())
}

fn print_summary(counts: &BTreeMap<&'static str, u64>, dry_run: bool) {
    let total: u64 = counts.values().sum();
    let header = if dry_run {
        "Backfill (dry run) — would have touched:"
    } else {
        "Backfill complete — touched:"
    };
    println!("{header}");
    if counts.is_empty() {
        println!("  (no rows)");
        return;
    }
    let width = counts.keys().map(|k| k.len()).max().unwrap_or(4);
    for (kind, n) in counts {
        println!("  {kind:<width$}  {n}", kind = kind, width = width);
    }
    println!("  {dash:<width$}  ----", dash = "", width = width);
    println!("  {total_label:<width$}  {total}", total_label = "total", width = width);
}
