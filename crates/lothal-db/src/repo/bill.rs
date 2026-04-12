use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use lothal_core::ontology::bill::{Bill, BillLineItem, LineItemCategory};
use lothal_core::ontology::utility::{RateSchedule, RateTier, RateType, UtilityAccount, UtilityType};
use lothal_core::temporal::BillingPeriod;
use lothal_core::units::Usd;

// ---------------------------------------------------------------------------
// Bill
// ---------------------------------------------------------------------------

/// Insert a bill and all its line items in a single transaction.
pub async fn insert_bill(pool: &PgPool, bill: &Bill) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"INSERT INTO bills (id, account_id, period_start, period_end,
                              statement_date, due_date, total_usage, usage_unit,
                              total_amount, source_file, notes, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"#,
    )
    .bind(bill.id)
    .bind(bill.account_id)
    .bind(bill.period.range.start)
    .bind(bill.period.range.end)
    .bind(bill.statement_date)
    .bind(bill.due_date)
    .bind(bill.total_usage)
    .bind(&bill.usage_unit)
    .bind(bill.total_amount.value())
    .bind(&bill.source_file)
    .bind(&bill.notes)
    .bind(bill.created_at)
    .bind(bill.updated_at)
    .execute(&mut *tx)
    .await?;

    for item in &bill.line_items {
        sqlx::query(
            r#"INSERT INTO bill_line_items (id, bill_id, description, category,
                                            amount, usage, rate)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        )
        .bind(item.id)
        .bind(item.bill_id)
        .bind(&item.description)
        .bind(item.category.to_string())
        .bind(item.amount.value())
        .bind(item.usage)
        .bind(item.rate)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Fetch a bill with its line items joined.
pub async fn get_bill(pool: &PgPool, id: Uuid) -> Result<Option<Bill>, sqlx::Error> {
    let bill_row = sqlx::query(
        "SELECT id, account_id, period_start, period_end, statement_date, due_date,
                total_usage, usage_unit, total_amount, source_file, notes,
                created_at, updated_at
         FROM bills WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = bill_row else {
        return Ok(None);
    };

    let bill_id: Uuid = {
        use sqlx::Row;
        row.get("id")
    };

    let item_rows = sqlx::query(
        "SELECT id, bill_id, description, category, amount, usage, rate
         FROM bill_line_items WHERE bill_id = $1 ORDER BY id",
    )
    .bind(bill_id)
    .fetch_all(pool)
    .await?;

    let line_items: Vec<BillLineItem> = item_rows.iter().map(line_item_from_row).collect();
    let mut bill = bill_from_row(&row);
    bill.line_items = line_items;

    Ok(Some(bill))
}

pub async fn list_bills_by_account(
    pool: &PgPool,
    account_id: Uuid,
) -> Result<Vec<Bill>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, account_id, period_start, period_end, statement_date, due_date,
                total_usage, usage_unit, total_amount, source_file, notes,
                created_at, updated_at
         FROM bills WHERE account_id = $1 ORDER BY period_start",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(bill_from_row).collect())
}

pub async fn list_bills_by_account_and_range(
    pool: &PgPool,
    account_id: Uuid,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Vec<Bill>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, account_id, period_start, period_end, statement_date, due_date,
                total_usage, usage_unit, total_amount, source_file, notes,
                created_at, updated_at
         FROM bills
         WHERE account_id = $1 AND period_start >= $2 AND period_end <= $3
         ORDER BY period_start",
    )
    .bind(account_id)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(bill_from_row).collect())
}

fn bill_from_row(row: &sqlx::postgres::PgRow) -> Bill {
    use sqlx::Row;
    let period_start: NaiveDate = row.get("period_start");
    let period_end: NaiveDate = row.get("period_end");
    Bill {
        id: row.get("id"),
        account_id: row.get("account_id"),
        period: BillingPeriod::new(period_start, period_end),
        statement_date: row.get("statement_date"),
        due_date: row.get("due_date"),
        total_usage: row.get("total_usage"),
        usage_unit: row.get("usage_unit"),
        total_amount: Usd::new(row.get("total_amount")),
        line_items: Vec::new(), // populated separately when needed
        source_file: row.get("source_file"),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn line_item_from_row(row: &sqlx::postgres::PgRow) -> BillLineItem {
    use sqlx::Row;
    let category_str: String = row.get("category");
    BillLineItem {
        id: row.get("id"),
        bill_id: row.get("bill_id"),
        description: row.get("description"),
        category: parse_line_item_category(&category_str),
        amount: Usd::new(row.get("amount")),
        usage: row.get("usage"),
        rate: row.get("rate"),
    }
}

fn parse_line_item_category(s: &str) -> LineItemCategory {
    match s.to_lowercase().as_str() {
        "base charge" => LineItemCategory::BaseCharge,
        "energy charge" => LineItemCategory::EnergyCharge,
        "delivery charge" => LineItemCategory::DeliveryCharge,
        "fuel cost adjustment" => LineItemCategory::FuelCostAdjustment,
        "demand charge" => LineItemCategory::DemandCharge,
        "rider charge" => LineItemCategory::RiderCharge,
        "tax" => LineItemCategory::Tax,
        "fee" => LineItemCategory::Fee,
        "credit" => LineItemCategory::Credit,
        _ => LineItemCategory::Other,
    }
}

// ---------------------------------------------------------------------------
// UtilityAccount
// ---------------------------------------------------------------------------

pub async fn insert_utility_account(
    pool: &PgPool,
    account: &UtilityAccount,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO utility_accounts (id, site_id, provider_name, utility_type,
                                         account_number, meter_id, is_active,
                                         created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
    )
    .bind(account.id)
    .bind(account.site_id)
    .bind(&account.provider_name)
    .bind(account.utility_type.to_string())
    .bind(&account.account_number)
    .bind(&account.meter_id)
    .bind(account.is_active)
    .bind(account.created_at)
    .bind(account.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_utility_account(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<UtilityAccount>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, site_id, provider_name, utility_type, account_number,
                meter_id, is_active, created_at, updated_at
         FROM utility_accounts WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| utility_account_from_row(&r)))
}

pub async fn list_utility_accounts_by_site(
    pool: &PgPool,
    site_id: Uuid,
) -> Result<Vec<UtilityAccount>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, site_id, provider_name, utility_type, account_number,
                meter_id, is_active, created_at, updated_at
         FROM utility_accounts WHERE site_id = $1 ORDER BY provider_name",
    )
    .bind(site_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(utility_account_from_row).collect())
}

fn utility_account_from_row(row: &sqlx::postgres::PgRow) -> UtilityAccount {
    use sqlx::Row;
    let ut_str: String = row.get("utility_type");
    UtilityAccount {
        id: row.get("id"),
        site_id: row.get("site_id"),
        provider_name: row.get("provider_name"),
        utility_type: ut_str.parse::<UtilityType>().unwrap_or(UtilityType::Electric),
        account_number: row.get("account_number"),
        meter_id: row.get("meter_id"),
        is_active: row.get("is_active"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

// ---------------------------------------------------------------------------
// RateSchedule
// ---------------------------------------------------------------------------

/// Insert a rate schedule and all its tiers in a single transaction.
pub async fn insert_rate_schedule(
    pool: &PgPool,
    schedule: &RateSchedule,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"INSERT INTO rate_schedules (id, account_id, name, rate_type,
                                       effective_from, effective_until,
                                       base_charge, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
    )
    .bind(schedule.id)
    .bind(schedule.account_id)
    .bind(&schedule.name)
    .bind(schedule.rate_type.to_string())
    .bind(schedule.effective_from)
    .bind(schedule.effective_until)
    .bind(schedule.base_charge.value())
    .bind(schedule.created_at)
    .bind(schedule.updated_at)
    .execute(&mut *tx)
    .await?;

    for (idx, tier) in schedule.tiers.iter().enumerate() {
        sqlx::query(
            r#"INSERT INTO rate_tiers (schedule_id, tier_order, label, lower_limit,
                                       upper_limit, rate_per_unit, peak_hours)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        )
        .bind(schedule.id)
        .bind(idx as i32)
        .bind(&tier.label)
        .bind(tier.lower_limit)
        .bind(tier.upper_limit)
        .bind(tier.rate_per_unit.value())
        .bind(&tier.peak_hours)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Get the currently active rate schedule for a given account (most recent
/// with effective_from <= today and effective_until is null or >= today).
pub async fn get_active_rate_schedule(
    pool: &PgPool,
    account_id: Uuid,
) -> Result<Option<RateSchedule>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT id, account_id, name, rate_type, effective_from, effective_until,
                  base_charge, created_at, updated_at
           FROM rate_schedules
           WHERE account_id = $1
             AND effective_from <= CURRENT_DATE
             AND (effective_until IS NULL OR effective_until >= CURRENT_DATE)
           ORDER BY effective_from DESC
           LIMIT 1"#,
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await?;

    let Some(sched_row) = row else {
        return Ok(None);
    };

    let schedule_id: Uuid = {
        use sqlx::Row;
        sched_row.get("id")
    };

    let tier_rows = sqlx::query(
        "SELECT label, lower_limit, upper_limit, rate_per_unit, peak_hours
         FROM rate_tiers WHERE schedule_id = $1 ORDER BY tier_order",
    )
    .bind(schedule_id)
    .fetch_all(pool)
    .await?;

    let tiers: Vec<RateTier> = tier_rows.iter().map(tier_from_row).collect();
    let mut schedule = rate_schedule_from_row(&sched_row);
    schedule.tiers = tiers;

    Ok(Some(schedule))
}

fn rate_schedule_from_row(row: &sqlx::postgres::PgRow) -> RateSchedule {
    use sqlx::Row;
    let rt_str: String = row.get("rate_type");
    RateSchedule {
        id: row.get("id"),
        account_id: row.get("account_id"),
        name: row.get("name"),
        rate_type: parse_rate_type(&rt_str),
        effective_from: row.get("effective_from"),
        effective_until: row.get("effective_until"),
        base_charge: Usd::new(row.get("base_charge")),
        tiers: Vec::new(), // populated separately when needed
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn tier_from_row(row: &sqlx::postgres::PgRow) -> RateTier {
    use sqlx::Row;
    RateTier {
        label: row.get("label"),
        lower_limit: row.get("lower_limit"),
        upper_limit: row.get("upper_limit"),
        rate_per_unit: Usd::new(row.get("rate_per_unit")),
        peak_hours: row.get("peak_hours"),
    }
}

fn parse_rate_type(s: &str) -> RateType {
    match s.to_lowercase().as_str() {
        "flat" => RateType::Flat,
        "tiered" => RateType::Tiered,
        "time-of-use" | "time_of_use" | "tou" => RateType::TimeOfUse,
        "demand" => RateType::Demand,
        _ => RateType::Flat,
    }
}
