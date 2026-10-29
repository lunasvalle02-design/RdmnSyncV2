//! Formato de cantidades para mostrar al usuario.

/// 1.5 → "1.5 h"; 2.0 → "2 h"
pub fn hours(h: f64) -> String {
    let s = format!("{h:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{s} h")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hours_without_trailing_zeros() {
        assert_eq!(hours(2.0), "2 h");
        assert_eq!(hours(1.5), "1.5 h");
        assert_eq!(hours(0.25), "0.25 h");
        assert_eq!(hours(1.333), "1.33 h");
    }
}
