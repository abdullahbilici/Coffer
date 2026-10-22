//! Payment schedules and totals for the assistant, so the model never does arithmetic.
//! Mirrors `src/lib/schedule.ts` and `addMonths` in `src/lib/dates.ts`; dates are `YYYY-MM-DD`
//! and compare as strings.

use std::collections::BTreeMap;

use crate::db::payments::Payment;

/// Amounts in hundredths per currency code, never converted.
pub type Totals = BTreeMap<String, i64>;

fn parse(date: &str) -> Option<(i64, i64, i64)> {
    let mut parts = date.splitn(3, '-').map(|p| p.parse::<i64>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

fn days_in_month(year: i64, month: i64) -> i64 {
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 31,
    }
}

/// `months` after `date` on the same day, clamped to the month's length (Jan 31 + 1 = Feb 28).
pub fn add_months(date: &str, months: i64) -> String {
    let Some((year, month, day)) = parse(date) else {
        return date.to_string();
    };
    let index = year * 12 + month - 1 + months;
    let (y, m) = (index.div_euclid(12), index.rem_euclid(12) + 1);
    format!("{y:04}-{m:02}-{:02}", day.min(days_in_month(y, m)))
}

/// `days` after `date` (negative goes back).
pub fn add_days(date: &str, days: i64) -> String {
    let Some((year, month, day)) = parse(date) else {
        return date.to_string();
    };
    // Howard Hinnant's days_from_civil and civil_from_days.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let z = era * 146_097 + doe + days;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The amount due on `date`: the latest price that started on or before it.
pub fn price_on(payment: &Payment, date: &str) -> i64 {
    let mut amount = payment.prices.first().map_or(0, |p| p.amount);
    for price in &payment.prices {
        if price.from.as_str() <= date {
            amount = price.amount;
        }
    }
    amount
}

/// Due dates within [from, to), stopping at the cancel date.
pub fn due_dates(payment: &Payment, from: &str, to: &str) -> Vec<String> {
    let end = match payment.cancel_date.as_deref() {
        Some(cancel) if cancel < to => cancel,
        _ => to,
    };
    let step = match payment.cycle.as_str() {
        "monthly" => 1,
        "yearly" => 12,
        _ => {
            let date = payment.start_date.as_str();
            return if date >= from && date < end {
                vec![date.to_string()]
            } else {
                Vec::new()
            };
        }
    };
    let mut dates = Vec::new();
    for n in (0..).step_by(step) {
        let date = add_months(&payment.start_date, n);
        if date.as_str() >= end {
            break;
        }
        if date.as_str() >= from {
            dates.push(date);
        }
    }
    dates
}

/// The next due date on or after `today`, if any.
pub fn next_due(payment: &Payment, today: &str) -> Option<String> {
    due_dates(payment, today, &add_months(today, 13))
        .into_iter()
        .next()
}

/// Whether the payment still recurs after `today` (started or not, and not cancelled by then).
pub fn is_active(payment: &Payment, today: &str) -> bool {
    payment.cycle != "once" && payment.cancel_date.as_deref().is_none_or(|c| c > today)
}

/// What everything due within [from, to) adds up to.
pub fn total_due<'a>(
    payments: impl IntoIterator<Item = &'a Payment>,
    from: &str,
    to: &str,
) -> Totals {
    let mut totals = Totals::new();
    for payment in payments {
        for date in due_dates(payment, from, to) {
            *totals.entry(payment.currency.clone()).or_default() += price_on(payment, &date);
        }
    }
    totals
}

/// The recurring cost per month at today's prices: monthly payments plus a twelfth of yearly
/// ones, for payments still active. One-time payments don't recur and are left out.
pub fn monthly_rate<'a>(payments: impl IntoIterator<Item = &'a Payment>, today: &str) -> Totals {
    let mut totals = Totals::new();
    for payment in payments.into_iter().filter(|p| is_active(p, today)) {
        let price = price_on(payment, today);
        let monthly = if payment.cycle == "yearly" {
            (price as f64 / 12.0).round() as i64
        } else {
            price
        };
        *totals.entry(payment.currency.clone()).or_default() += monthly;
    }
    totals
}

/// `12.50 EUR`; currencies are codes, the model formats them for the user.
pub fn money(amount: i64, currency: &str) -> String {
    let sign = if amount < 0 { "-" } else { "" };
    let amount = amount.abs();
    format!("{sign}{}.{:02} {currency}", amount / 100, amount % 100)
}

/// `12.50 EUR + 3.00 USD`, or `none`.
pub fn totals_text(totals: &Totals) -> String {
    if totals.is_empty() {
        return "none".to_string();
    }
    totals
        .iter()
        .map(|(currency, amount)| money(*amount, currency))
        .collect::<Vec<_>>()
        .join(" + ")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::payments::Price;

    pub fn payment(
        cycle: &str,
        start: &str,
        cancel: Option<&str>,
        prices: &[(&str, i64)],
    ) -> Payment {
        Payment {
            id: 1,
            project_id: None,
            name: "Hosting".into(),
            currency: "EUR".into(),
            cycle: cycle.into(),
            start_date: start.into(),
            cancel_date: cancel.map(Into::into),
            prices: prices
                .iter()
                .map(|(from, amount)| Price {
                    from: (*from).into(),
                    amount: *amount,
                })
                .collect(),
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn adds_months_like_the_frontend() {
        assert_eq!(add_months("2026-01-31", 1), "2026-02-28");
        assert_eq!(add_months("2024-01-31", 1), "2024-02-29");
        assert_eq!(add_months("2026-11-15", 3), "2027-02-15");
        assert_eq!(add_months("2026-03-31", -1), "2026-02-28");
        assert_eq!(add_days("2026-12-31", 1), "2027-01-01");
        assert_eq!(add_days("2024-03-01", -1), "2024-02-29");
        assert_eq!(add_days("2026-10-23", 0), "2026-10-23");
    }

    #[test]
    fn schedules_with_price_changes_and_cancel() {
        let p = payment(
            "monthly",
            "2026-01-31",
            Some("2026-05-01"),
            &[("2026-01-31", 1000), ("2026-03-31", 1500)],
        );
        assert_eq!(
            due_dates(&p, "2026-01-01", "2027-01-01"),
            ["2026-01-31", "2026-02-28", "2026-03-31", "2026-04-30"]
        );
        let totals = total_due([&p], "2026-01-01", "2027-01-01");
        assert_eq!(totals_text(&totals), "50.00 EUR");
        assert_eq!(next_due(&p, "2026-03-01").as_deref(), Some("2026-03-31"));
        assert_eq!(next_due(&p, "2026-06-01"), None);
        assert!(!is_active(&p, "2026-05-01"));
    }

    #[test]
    fn monthly_rate_spreads_yearly_and_skips_one_time() {
        let monthly = payment("monthly", "2026-01-01", None, &[("2026-01-01", 1249)]);
        let yearly = payment("yearly", "2025-06-01", None, &[("2025-06-01", 9900)]);
        let once = payment("once", "2026-02-01", None, &[("2026-02-01", 50000)]);
        let rate = monthly_rate([&monthly, &yearly, &once], "2026-10-23");
        assert_eq!(totals_text(&rate), "20.74 EUR");
        assert_eq!(money(-5, "USD"), "-0.05 USD");
        assert_eq!(totals_text(&Totals::new()), "none");
    }
}
