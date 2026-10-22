use serde::Deserialize;
use tauri::State;

use super::{required_name, valid_date};
use crate::{
    db::payments::{self, Payment, PaymentRecord},
    error::{AppError, AppResult},
    state::AppState,
    time::now_millis,
};

const CYCLES: [&str; 3] = ["monthly", "yearly", "once"];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceInput {
    /// `YYYY-MM-DD`; ignored when creating, where the price starts with the payment.
    from: String,
    /// Integer hundredths.
    amount: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePaymentInput {
    /// `None` creates a new payment.
    id: Option<i64>,
    project_id: Option<i64>,
    name: String,
    currency: String,
    cycle: String,
    start_date: String,
    cancel_date: Option<String>,
    /// Required when creating; when editing, `Some` records a price change.
    price: Option<PriceInput>,
}

#[tauri::command]
pub fn save_payment(state: State<AppState>, input: SavePaymentInput) -> AppResult<Payment> {
    state.session().require_unlocked()?;
    let name = required_name(&input.name, "Name")?;
    let currency = input.currency.trim().to_uppercase();
    if currency.len() != 3 || !currency.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(AppError::Invalid(
            "Currency must be a three-letter code".to_string(),
        ));
    }
    if !CYCLES.contains(&input.cycle.as_str()) {
        return Err(AppError::Invalid(format!("Unknown cycle: {}", input.cycle)));
    }
    let start_date = valid_date(&input.start_date)?;
    let cancel_date = input.cancel_date.as_deref().map(valid_date).transpose()?;
    if cancel_date.is_some_and(|cancel| cancel <= start_date) {
        return Err(AppError::Invalid(
            "Cancel date must be after the start date".to_string(),
        ));
    }
    if let Some(price) = &input.price {
        valid_date(&price.from)?;
        if price.amount < 0 {
            return Err(AppError::Invalid("Amount can't be negative".to_string()));
        }
    }

    let record = PaymentRecord {
        project_id: input.project_id,
        name: &name,
        currency: &currency,
        cycle: &input.cycle,
        start_date,
        cancel_date,
    };
    let conn = state.db();
    let now = now_millis();
    let id = match (input.id, input.price) {
        (Some(id), price) => {
            let price = price.as_ref().map(|p| (p.from.as_str(), p.amount));
            payments::update(&conn, id, &record, price, now)?;
            id
        }
        (None, Some(price)) => payments::insert(&conn, &record, price.amount, now)?,
        (None, None) => return Err(AppError::Invalid("Amount is required".to_string())),
    };
    payments::get(&conn, id)
}

#[tauri::command]
pub fn delete_payment(state: State<AppState>, id: i64) -> AppResult<()> {
    state.session().require_unlocked()?;
    payments::delete(&state.db(), id)
}
