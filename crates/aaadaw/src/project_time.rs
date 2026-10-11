use aaadaw_core::{FrameRate, TimebaseError};

pub(crate) fn format_project_time(seconds: f64) -> String {
    if !seconds.is_finite() {
        return "Unavailable".to_owned();
    }
    let sign = if seconds < 0.0 { "-" } else { "" };
    let adjusted = seconds.abs() + 1e-8;
    let milliseconds = (adjusted * 1000.0).floor();
    if milliseconds < u64::MAX as f64 {
        let milliseconds = milliseconds as u64;
        let whole = milliseconds / 1000;
        if whole < 3600 {
            format!(
                "{sign}{}:{:02}.{:03}",
                whole / 60,
                whole % 60,
                milliseconds % 1000
            )
        } else {
            format!(
                "{sign}{}:{:02}:{:02}.{:03}",
                whole / 3600,
                whole / 60 % 60,
                whole % 60,
                milliseconds % 1000
            )
        }
    } else {
        format!("{sign}{:.3}", seconds.abs())
    }
}

pub(crate) fn parse_project_time(text: &str) -> Result<f64, &'static str> {
    let text = text.trim();
    let (sign, unsigned) = if let Some(value) = text.strip_prefix('-') {
        (-1.0, value)
    } else if let Some(value) = text.strip_prefix('+') {
        (1.0, value)
    } else {
        (1.0, text)
    };
    let fields = unsigned.split(':').collect::<Vec<_>>();
    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|field| field.is_empty()) {
        return Err("Invalid project start time");
    }
    let values = fields
        .iter()
        .map(|field| {
            field
                .parse::<f64>()
                .map_err(|_| "Invalid project start time")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if values
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
    {
        return Err("Invalid project start time");
    }
    let seconds = match values.as_slice() {
        [seconds] => *seconds,
        [minutes, seconds] if *seconds < 60.0 => minutes * 60.0 + seconds,
        [hours, minutes, seconds] if *minutes < 60.0 && *seconds < 60.0 => {
            hours * 3600.0 + minutes * 60.0 + seconds
        }
        _ => return Err("Invalid project start time"),
    } * sign;
    if seconds.is_finite() {
        Ok(seconds)
    } else {
        Err("Invalid project start time")
    }
}

pub(crate) fn format_project_timecode(
    rate: FrameRate,
    seconds: f64,
) -> Result<String, TimebaseError> {
    if !seconds.is_finite() {
        return Err(TimebaseError::PositionOutOfRange);
    }
    let label = rate.format_timecode(seconds.abs())?;
    Ok(if seconds < 0.0 {
        format!("-{label}")
    } else {
        label
    })
}

pub(crate) fn parse_project_timecode(rate: FrameRate, text: &str) -> Result<f64, TimebaseError> {
    let text = text.trim();
    let (sign, unsigned) = if let Some(value) = text.strip_prefix('-') {
        (-1.0, value)
    } else if let Some(value) = text.strip_prefix('+') {
        (1.0, value)
    } else {
        (1.0, text)
    };
    Ok(sign * rate.parse_timecode(unsigned)?)
}

#[cfg(test)]
mod tests {
    use super::{
        format_project_time, format_project_timecode, parse_project_time, parse_project_timecode,
    };
    use aaadaw_core::FrameRate;

    #[test]
    fn signed_project_time_formats_and_parses_native_clock_values() {
        for (text, seconds) in [
            ("0:00.000", 0.0),
            ("0:10.000", 10.0),
            ("-0:05.000", -5.0),
            ("1:02:03.125", 3_723.125),
        ] {
            assert_eq!(parse_project_time(text), Ok(seconds));
            assert_eq!(format_project_time(seconds), text);
        }
    }

    #[test]
    fn project_time_parser_rejects_malformed_and_non_finite_values() {
        for text in ["", "1:", "1:60.000", "1:2:60", "NaN", "inf", "1:2:3:4"] {
            assert!(parse_project_time(text).is_err(), "accepted {text:?}");
        }
    }

    #[test]
    fn signed_timecode_formats_and_parses_negative_project_clock_values() {
        let rate = FrameRate::Fps30;
        let text = "-00:00:05:00";
        assert_eq!(format_project_timecode(rate, -5.0).unwrap(), text);
        assert_eq!(parse_project_timecode(rate, text).unwrap(), -5.0);
    }
}
