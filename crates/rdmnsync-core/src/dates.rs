//! Utilidades de fechas (formato ISO de la API de Redmine y presentación en español).

use chrono::{Datelike, Duration, Local, NaiveDate};

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

pub fn week_of(d: NaiveDate) -> (NaiveDate, NaiveDate) {
    let monday = d - Duration::days(d.weekday().num_days_from_monday() as i64);
    (monday, monday + Duration::days(6))
}

pub fn month_of(d: NaiveDate) -> (NaiveDate, NaiveDate) {
    let first = d.with_day(1).unwrap_or(d);
    let next = if first.month() == 12 {
        NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
    };
    (first, next.map(|n| n - Duration::days(1)).unwrap_or(first))
}

pub fn parse(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
}

pub fn iso(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub fn weekday_es(d: NaiveDate) -> &'static str {
    ["Lunes", "Martes", "Miércoles", "Jueves", "Viernes", "Sábado", "Domingo"]
        [d.weekday().num_days_from_monday() as usize]
}

/// "2026-10-28" → "28/10/2026"
pub fn pretty(s: &str) -> String {
    parse(s).map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_else(|| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        parse(s).unwrap()
    }

    #[test]
    fn week_and_month_ranges() {
        assert_eq!(week_of(d("2026-10-29")), (d("2026-10-26"), d("2026-11-01")));
        assert_eq!(month_of(d("2026-12-15")), (d("2026-12-01"), d("2026-12-31")));
        assert_eq!(month_of(d("2028-02-10")), (d("2028-02-01"), d("2028-02-29")));
    }

    #[test]
    fn formatting() {
        assert_eq!(iso(d(" 2026-01-05 ")), "2026-01-05");
        assert_eq!(pretty("2026-10-28"), "28/10/2026");
        assert_eq!(pretty("sin fecha"), "sin fecha");
        assert_eq!(weekday_es(d("2026-10-29")), "Jueves");
    }
}
