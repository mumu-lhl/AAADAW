//! Factory Default curve sampled from REAPER 7.82's public conversion API.
//! Widget positions use whole units, or tenths with Shift; each corresponds to
//! an independently measured API input. Forward display geometry interpolates
//! the separate dB-to-position table, retaining arbitrary numeric gain values.

use super::fader_data::{DB_TO_POSITION, MIN_FORWARD_DB, POSITION_TO_DB};

pub(super) const MAX_DB: f32 = 12.0;
pub(super) const SILENCE_DB: f32 = -1000.0;

pub(super) fn to_position(db: f32) -> f32 {
    if !db.is_finite() || f64::from(db) < MIN_FORWARD_DB {
        return 0.0;
    }
    let index = ((f64::from(db.min(MAX_DB)) - MIN_FORWARD_DB) * 10.0)
        .clamp(0.0, (DB_TO_POSITION.len() - 1) as f64);
    let low = index.floor() as usize;
    let high = (low + 1).min(DB_TO_POSITION.len() - 1);
    let fraction = index - low as f64;
    (DB_TO_POSITION[low] + fraction * (DB_TO_POSITION[high] - DB_TO_POSITION[low])) as f32
}

pub(super) fn from_position(position: f32) -> f32 {
    let index = (position.clamp(0.0, 1000.0) * 10.0).round() as usize;
    POSITION_TO_DB[index.min(POSITION_TO_DB.len() - 1)] as f32
}

pub(super) fn format_db(db: f32) -> String {
    if db <= SILENCE_DB {
        "-inf".to_owned()
    } else {
        format!("{db:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_positions_match_independent_reference_anchors() {
        for (db, position) in [
            (-72.0, 50.156251),
            (-60.0, 79.482343),
            (-24.0, 313.298690),
            (-6.0, 592.848336),
            (0.0, 716.0),
            (6.0, 852.286980),
            (12.0, 1000.0),
        ] {
            assert!((f64::from(to_position(db)) - position).abs() < 0.0002);
        }
        for (position, db) in [
            (50.0, -72.081314),
            (100.0, -54.011918),
            (300.0, -25.161916),
            (500.0, -11.059349),
            (700.0, -0.753197),
            (716.0, 0.0),
            (1000.0, 12.0),
        ] {
            assert!((f64::from(from_position(position)) - db).abs() < 0.0001);
        }
    }

    #[test]
    fn zero_endpoint_is_finite_silent_and_curve_is_monotonic() {
        assert_eq!(from_position(0.0), SILENCE_DB);
        assert_eq!(10.0_f32.powf(from_position(0.0) / 20.0), 0.0);
        assert_eq!(format_db(SILENCE_DB), "-inf");
        assert_eq!(to_position(SILENCE_DB), 0.0);
        let mut last = SILENCE_DB;
        for index in 0..=10000 {
            let value = from_position(index as f32 / 10.0);
            assert!(value.is_finite() && value >= last);
            last = value;
        }
        assert_eq!(from_position(2000.0), MAX_DB);
    }
}
