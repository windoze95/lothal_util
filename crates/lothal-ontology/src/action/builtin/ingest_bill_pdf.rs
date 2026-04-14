//! `ingest_bill_pdf` — decode a base64 PDF, shell out to `pdftotext`, delegate
//! to the `bill_extraction` [`LlmFunction`][crate::llm_function::LlmFunction]
//! for structured-field extraction, and persist the resulting `Bill` +
//! `BillLineItem` rows.
//!
//! The ontology crate cannot depend on `lothal-db` (that would be a cycle),
//! so this action inlines the subset of `lothal-db::bill::insert_bill` it
//! needs via the local `indexer` helpers. The prompt + extraction schema
//! live in the `bill_extraction` LLM function.

use async_trait::async_trait;
use chrono::NaiveDate;
use serde_json::json;
use uuid::Uuid;

use lothal_core::ontology::bill::{Bill, BillLineItem, LineItemCategory};
use lothal_core::units::Usd;

use crate::action::{Action, ActionCtx, ActionError};
use crate::{indexer, Describe, EventSpec, LinkSpec, ObjectRef};

use super::{subjects_from_input, truncate};

pub struct IngestBillPdf;

/// Hard cap on decoded PDF size to avoid hostile input blowing up memory.
const MAX_DECODED_BYTES: usize = 10 * 1024 * 1024;

#[async_trait]
impl Action for IngestBillPdf {
    fn name(&self) -> &'static str {
        "ingest_bill_pdf"
    }

    fn description(&self) -> &'static str {
        "Decode a base64 utility bill PDF, extract structured fields via Claude, \
         and persist a Bill + line items linked to the utility_account subject."
    }

    fn applicable_kinds(&self) -> &'static [&'static str] {
        &["utility_account"]
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["pdf_base64"],
            "properties": {
                "pdf_base64": {
                    "type": "string",
                    "description": "Base64-encoded bill PDF (decoded cap 10 MB)"
                },
                "filename": {"type": "string"}
            }
        })
    }

    fn output_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "required": [
                "bill_id", "period_start", "period_end",
                "total_amount_usd", "line_item_count", "event_id"
            ],
            "properties": {
                "bill_id":          {"type": "string", "format": "uuid"},
                "period_start":     {"type": "string", "format": "date"},
                "period_end":       {"type": "string", "format": "date"},
                "total_amount_usd": {"type": "number"},
                "line_item_count":  {"type": "integer"},
                "event_id":         {"type": "string", "format": "uuid"}
            }
        })
    }

    async fn run(
        &self,
        ctx: &ActionCtx,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ActionError> {
        let functions = ctx.llm_functions.as_ref().ok_or_else(|| {
            ActionError::Other(anyhow::anyhow!("LlmFunctionRegistry not configured"))
        })?;

        let subjects = subjects_from_input(&input)?;
        let account_ref = subjects.first().ok_or_else(|| {
            ActionError::InvalidInput("ingest_bill_pdf requires one utility_account subject".into())
        })?;
        if account_ref.kind != "utility_account" {
            return Err(ActionError::NotApplicable(account_ref.kind.clone()));
        }

        // 1. Decode and size-check the PDF.
        let pdf_b64 = input
            .get("pdf_base64")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionError::InvalidInput("pdf_base64 is required".into()))?;
        let filename = input
            .get("filename")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let pdf_bytes = base64_decode(pdf_b64)
            .map_err(|_| ActionError::InvalidInput("pdf_base64 not valid base64".into()))?;
        if pdf_bytes.len() > MAX_DECODED_BYTES {
            return Err(ActionError::InvalidInput("pdf exceeds 10MB decoded".into()));
        }

        // 2. Load the account to resolve site_id + confirm we support its type.
        let (account_site_id, utility_type) = load_utility_account(ctx, account_ref.id).await?;
        validate_utility_type(&utility_type)?;

        // 3. Extract text via `pdftotext -layout`.
        let pdf_text = extract_text_from_mem(&pdf_bytes)
            .map_err(|e| ActionError::Other(anyhow::anyhow!("pdf parse failed: {e}")))?;

        // 4. Delegate structured extraction to the `bill_extraction` LLM function.
        let call = functions
            .invoke(
                "bill_extraction",
                &ctx.invoked_by,
                ctx.pool.clone(),
                json!({
                    "pdf_text": pdf_text,
                    "utility_type": utility_type,
                }),
                Some(ctx.run_id),
                None,
            )
            .await
            .map_err(|e| ActionError::Other(anyhow::anyhow!("bill_extraction function: {e}")))?;
        let extracted: serde_json::Value = call
            .output
            .as_ref()
            .map(|v| v.0.clone())
            .ok_or_else(|| ActionError::Other(anyhow::anyhow!("bill_extraction returned no output")))?;

        // 5. Convert → typed Bill and persist in one transaction.
        let bill = build_bill_from_extraction(&extracted, account_ref.id, filename.as_deref())?;
        persist_bill(ctx, &bill, account_ref, account_site_id).await?;

        // 6. Emit the business-level `bill_ingested` event.
        let summary = format!(
            "Bill {} - {}, ${:.2}",
            bill.period.range.start,
            bill.period.range.end,
            bill.total_amount.value()
        );
        let event_id = ctx
            .emit_event(EventSpec {
                kind: "bill_ingested".into(),
                site_id: account_site_id,
                subjects: vec![
                    ObjectRef::new(Bill::KIND, bill.id),
                    account_ref.clone(),
                ],
                summary: truncate(&summary, 160),
                severity: Some("info".into()),
                properties: json!({
                    "bill_id": bill.id,
                    "account_id": bill.account_id,
                    "period_start": bill.period.range.start.to_string(),
                    "period_end": bill.period.range.end.to_string(),
                    "total_amount_usd": bill.total_amount.value(),
                    "total_usage": bill.total_usage,
                    "usage_unit": bill.usage_unit,
                    "line_item_count": bill.line_items.len(),
                    "utility_type": utility_type,
                }),
                source: "action:ingest_bill_pdf".into(),
            })
            .await?;

        Ok(json!({
            "bill_id": bill.id,
            "period_start": bill.period.range.start.to_string(),
            "period_end": bill.period.range.end.to_string(),
            "total_amount_usd": bill.total_amount.value(),
            "line_item_count": bill.line_items.len(),
            "event_id": event_id,
        }))
    }
}

/// Pull `(site_id, utility_type)` for the subject account. Using raw SQL keeps
/// this crate free of a `lothal-db` dependency (which would be a cycle).
async fn load_utility_account(
    ctx: &ActionCtx,
    account_id: Uuid,
) -> Result<(Option<Uuid>, String), ActionError> {
    let row: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT site_id, utility_type FROM utility_accounts WHERE id = $1",
    )
    .bind(account_id)
    .fetch_optional(&ctx.pool)
    .await?;

    let (site_id, utility_type) = row.ok_or_else(|| {
        ActionError::InvalidInput(format!("utility_account {account_id} not found"))
    })?;
    Ok((Some(site_id), utility_type))
}

/// Reject utility types we have no bill parser tuning for. The LLM can
/// technically parse any of them, but the caller asked us to gate here so
/// new providers fail loudly rather than silently.
fn validate_utility_type(ut: &str) -> Result<(), ActionError> {
    match ut.to_lowercase().as_str() {
        "electric" | "gas" | "water" | "sewer" => Ok(()),
        other => Err(ActionError::NotApplicable(format!(
            "utility_type {other} has no bill extractor"
        ))),
    }
}

/// Shell out to `pdftotext` (poppler) to extract text from PDF bytes.
///
/// The existing `lothal-ai::extract::extract_text_from_pdf` helper expects a
/// file path, so we write the decoded bytes to a tempfile first, then remove
/// the file before returning.
fn extract_text_from_mem(bytes: &[u8]) -> anyhow::Result<String> {
    use std::io::Write;
    use std::process::Command;

    if bytes.len() < 4 || &bytes[..4] != b"%PDF" {
        anyhow::bail!("input is not a PDF (missing %PDF magic)");
    }

    let tmp_path = std::env::temp_dir().join(format!("lothal_bill_{}.pdf", Uuid::new_v4()));
    {
        let mut f = std::fs::File::create(&tmp_path)
            .map_err(|e| anyhow::anyhow!("tempfile create: {e}"))?;
        f.write_all(bytes)
            .map_err(|e| anyhow::anyhow!("tempfile write: {e}"))?;
    }

    let output = Command::new("pdftotext")
        .arg("-layout")
        .arg(&tmp_path)
        .arg("-")
        .output();

    // Cleanup tempfile regardless of outcome.
    let _ = std::fs::remove_file(&tmp_path);

    let output = output.map_err(|e| anyhow::anyhow!("failed to run pdftotext: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("pdftotext exited with {}: {stderr}", output.status);
    }
    String::from_utf8(output.stdout).map_err(|e| anyhow::anyhow!("pdftotext utf-8: {e}"))
}

/// Convert the LLM's JSON into a typed `Bill` with child `BillLineItem`s.
fn build_bill_from_extraction(
    raw: &serde_json::Value,
    account_id: Uuid,
    filename: Option<&str>,
) -> Result<Bill, ActionError> {
    let period_start = read_date(raw, "period_start")?;
    let period_end = read_date(raw, "period_end")?;
    let statement_date = read_date(raw, "statement_date")?;
    let total_usage = raw
        .get("total_usage")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| ActionError::Other(anyhow::anyhow!("LLM response missing total_usage")))?;
    let usage_unit = raw
        .get("usage_unit")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let total_amount = raw
        .get("total_amount")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| ActionError::Other(anyhow::anyhow!("LLM response missing total_amount")))?;

    let mut bill = Bill::new(
        account_id,
        period_start,
        period_end,
        statement_date,
        total_usage,
        usage_unit,
        Usd::new(total_amount),
    );
    bill.source_file = filename.map(|s| s.to_string());
    bill.parse_method = Some("ingest_bill_pdf".into());

    let items = raw.get("line_items").and_then(|v| v.as_array());
    if let Some(arr) = items {
        for item in arr {
            let description = item
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let category = item
                .get("category")
                .and_then(|v| v.as_str())
                .map(parse_category)
                .unwrap_or(LineItemCategory::Other);
            let amount = item
                .get("amount")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let mut li = BillLineItem::new(
                bill.id,
                description,
                category,
                Usd::new(amount),
            );
            li.usage = item.get("usage").and_then(|v| v.as_f64());
            li.rate = item.get("rate").and_then(|v| v.as_f64());
            bill.line_items.push(li);
        }
    }

    Ok(bill)
}

fn read_date(raw: &serde_json::Value, key: &str) -> Result<NaiveDate, ActionError> {
    let s = raw
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| ActionError::Other(anyhow::anyhow!("LLM response missing `{key}`")))?;
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| {
        ActionError::Other(anyhow::anyhow!("invalid date for {key}: '{s}' ({e})"))
    })
}

fn parse_category(s: &str) -> LineItemCategory {
    match s.to_lowercase().replace(' ', "_").as_str() {
        "base_charge" => LineItemCategory::BaseCharge,
        "energy_charge" => LineItemCategory::EnergyCharge,
        "delivery_charge" => LineItemCategory::DeliveryCharge,
        "fuel_cost_adjustment" => LineItemCategory::FuelCostAdjustment,
        "demand_charge" => LineItemCategory::DemandCharge,
        "rider_charge" => LineItemCategory::RiderCharge,
        "tax" => LineItemCategory::Tax,
        "fee" => LineItemCategory::Fee,
        "credit" => LineItemCategory::Credit,
        _ => LineItemCategory::Other,
    }
}

/// Persist the bill + line items + indexer rows in a single transaction.
/// Mirrors the SQL shape of `lothal-db::bill::insert_bill` (duplicated here
/// because `lothal-ontology` can't depend on `lothal-db`).
async fn persist_bill(
    ctx: &ActionCtx,
    bill: &Bill,
    account_ref: &ObjectRef,
    _account_site_id: Option<Uuid>,
) -> Result<(), ActionError> {
    let mut tx = ctx.pool.begin().await?;

    sqlx::query(
        r#"INSERT INTO bills (id, account_id, period_start, period_end,
                              statement_date, due_date, total_usage, usage_unit,
                              total_amount, source_file, notes, created_at, updated_at,
                              parse_method, llm_model, llm_confidence)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                   $14, $15, $16)"#,
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
    .bind(&bill.parse_method)
    .bind(&bill.llm_model)
    .bind(bill.llm_confidence)
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

    indexer::upsert_object(&mut tx, bill).await?;
    indexer::upsert_link(
        &mut tx,
        LinkSpec::new(
            "issued_by",
            ObjectRef::new(Bill::KIND, bill.id),
            account_ref.clone(),
        ),
    )
    .await?;
    indexer::emit_event(
        &mut tx,
        EventSpec::record_registered(bill, "action:ingest_bill_pdf"),
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Minimal base64 decoder. Standard alphabet plus URL-safe aliases; whitespace
/// and `=` padding are tolerated. Ported from
/// `lothal-ai::extract::email::base64_decode` to avoid pulling in a crate
/// dependency for ~30 lines of logic.
fn base64_decode(input: &str) -> Result<Vec<u8>, &'static str> {
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;

    for &b in input.as_bytes() {
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' | b'\t' => continue,
            _ => return Err("invalid base64 byte"),
        };

        buf = (buf << 6) | u32::from(val);
        bits += 6;

        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip_basic() {
        let input = "aGVsbG8gd29ybGQ=";
        let out = base64_decode(input).unwrap();
        assert_eq!(out, b"hello world");
    }

    #[test]
    fn base64_rejects_garbage() {
        assert!(base64_decode("***not base64***").is_err());
    }

    #[test]
    fn build_bill_maps_fields_and_line_items() {
        let raw = json!({
            "period_start":   "2026-01-01",
            "period_end":     "2026-01-31",
            "statement_date": "2026-02-01",
            "total_usage":    1200.0,
            "usage_unit":     "kWh",
            "total_amount":   150.00,
            "line_items": [
                {"description": "Base charge", "category": "base_charge", "amount": 15.00},
                {"description": "Energy",      "category": "energy_charge", "amount": 120.00,
                 "usage": 1200.0, "rate": 0.10},
                {"description": "Tax",         "category": "tax", "amount": 15.00}
            ]
        });
        let account = Uuid::new_v4();
        let bill = build_bill_from_extraction(&raw, account, Some("test.pdf")).unwrap();
        assert_eq!(bill.account_id, account);
        assert_eq!(bill.line_items.len(), 3);
        assert_eq!(bill.total_amount.value(), 150.00);
        assert_eq!(bill.source_file.as_deref(), Some("test.pdf"));
        assert_eq!(bill.parse_method.as_deref(), Some("ingest_bill_pdf"));
    }

    #[test]
    fn validate_utility_type_accepts_common() {
        assert!(validate_utility_type("electric").is_ok());
        assert!(validate_utility_type("GAS").is_ok());
        assert!(matches!(
            validate_utility_type("trash"),
            Err(ActionError::NotApplicable(_))
        ));
    }

    #[test]
    fn parse_category_known_and_default() {
        assert_eq!(parse_category("base_charge"), LineItemCategory::BaseCharge);
        assert_eq!(parse_category("energy charge"), LineItemCategory::EnergyCharge);
        assert_eq!(parse_category("weird-thing"), LineItemCategory::Other);
    }
}
