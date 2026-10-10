use aaadaw_core::Project;

fn failure(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn beat_ticks(project: &Project, tick: f64) -> f64 {
    let meter = project.time_signature_at_tick(tick.floor() as u64);
    f64::from(project.settings().ppq()) * 4.0 / f64::from(meter.denominator())
}

fn position(project: &Project, sample: f64) -> Result<(u64, f64), String> {
    let tick = project.tick_at_sample_position(sample).map_err(failure)?;
    let position = project
        .musical_position_at_tick(tick.floor() as u64)
        .map_err(failure)?;
    let beat = f64::from(position.beat() - 1)
        + (position.tick_in_beat() as f64 + tick.fract()) / beat_ticks(project, tick);
    Ok((position.measure(), beat))
}

pub(super) fn format_position(project: &Project, sample: f64) -> Result<String, String> {
    let tick = project.tick_at_sample_position(sample).map_err(failure)?;
    let pos = project
        .musical_position_at_tick(tick.floor() as u64)
        .map_err(failure)?;
    let rounded = ((pos.tick_in_beat() as f64 + tick.fract()) / beat_ticks(project, tick) * 100.0)
        .round() as u64;
    if rounded == 100 {
        let next = tick - pos.tick_in_beat() as f64 - tick.fract() + beat_ticks(project, tick);
        let pos = project
            .musical_position_at_tick(next.round() as u64)
            .map_err(failure)?;
        return Ok(format!("{}.{}.00", pos.measure(), pos.beat()));
    }
    Ok(format!("{}.{}.{rounded:02}", pos.measure(), pos.beat()))
}

pub(super) fn format_length(project: &Project, start: f64, length: f64) -> Result<String, String> {
    let (start_bar, start_beat) = position(project, start)?;
    let (end_bar, end_beat) = position(project, start + length)?;
    let tick = project.tick_at_sample_position(start).map_err(failure)?;
    let numerator = i128::from(
        project
            .time_signature_at_tick(tick.floor() as u64)
            .numerator(),
    );
    let mut bars = i128::from(end_bar) - i128::from(start_bar);
    let mut hundredths = ((end_beat - start_beat) * 100.0).round() as i128;
    if hundredths < 0 {
        let borrowed = (-hundredths + numerator * 100 - 1) / (numerator * 100);
        bars -= borrowed;
        hundredths += borrowed * numerator * 100;
    }
    bars += hundredths / (numerator * 100);
    hundredths %= numerator * 100;
    Ok(format!(
        "{bars}.{}.{:02}",
        hundredths / 100,
        hundredths % 100
    ))
}

// Canonical nonnegative bar.beat.fraction inputs. Native permissive malformed,
// signed and trailing-text parsing is recorded separately and remains pending.
fn parse(text: &str) -> Result<(u64, u64, f64), String> {
    let fields: Vec<_> = text.trim().split('.').collect();
    if fields.len() > 3
        || fields
            .iter()
            .any(|field| field.is_empty() || !field.bytes().all(|c| c.is_ascii_digit()))
    {
        return Err("Enter bars.beats.fraction using nonnegative digits".to_owned());
    }
    let bars = fields[0].parse::<u64>().map_err(failure)?;
    let beats = fields
        .get(1)
        .map_or(Ok(0), |field| field.parse::<u64>())
        .map_err(failure)?;
    let fraction = fields
        .get(2)
        .map_or(Ok(0.0), |field| format!("0.{field}").parse::<f64>())
        .map_err(failure)?;
    Ok((bars, beats, fraction))
}

pub(super) fn parse_position(project: &Project, text: &str) -> Result<f64, String> {
    let (bars, beats, fraction) = parse(text)?;
    let tick = project
        .tick_at_musical_position(bars.max(1), 1, 0.0)
        .map_err(failure)?;
    let beats = if text.trim().contains('.') { beats } else { 1 };
    let tick = (tick + (beats as f64 - 1.0 + fraction) * beat_ticks(project, tick)).max(0.0);
    project.sample_at_tick_position(tick).map_err(failure)
}

pub(super) fn parse_length(project: &Project, text: &str, start: f64) -> Result<f64, String> {
    let (bars, beats, fraction) = parse(text)?;
    let tick = project.tick_at_sample_position(start).map_err(failure)?;
    let numerator = project
        .time_signature_at_tick(tick.floor() as u64)
        .numerator();
    let ticks =
        (bars as f64 * f64::from(numerator) + beats as f64 + fraction) * beat_ticks(project, tick);
    let tick = tick + ticks;
    let end = project.sample_at_tick_position(tick).map_err(failure)?;
    Ok(end - start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aaadaw_core::{DawAction, TimeSignature};
    fn project(bpm: f64, numerator: u32, denominator: u32) -> Project {
        let mut project = Project::new();
        project
            .apply(DawAction::SetTempo { start_tick: 0, bpm })
            .unwrap();
        project
            .apply(DawAction::SetTimeSignature {
                start_tick: 0,
                signature: TimeSignature::new(numerator, denominator).unwrap(),
            })
            .unwrap();
        project
    }
    #[test]
    fn native_constant_tempo_and_meter_formats() {
        for row in include_str!(
            "../../../../../docs/verification/reaper-parity/time-displays-reference.tsv"
        )
        .lines()
        .skip(2)
        {
            let columns: Vec<_> = row.split('\t').collect();
            if columns[6] != "2" {
                continue;
            }
            let project = project(
                columns[1].parse().unwrap(),
                columns[2].parse().unwrap(),
                columns[3].parse().unwrap(),
            );
            let start = columns[4].parse::<f64>().unwrap() * 48000.0;
            let length = columns[5].parse::<f64>().unwrap() * 48000.0;
            assert_eq!(
                format_position(&project, start).unwrap(),
                columns[7],
                "{}",
                columns[0]
            );
            assert_eq!(
                format_length(&project, start, length).unwrap(),
                columns[8],
                "{}",
                columns[0]
            );
        }
    }
    #[test]
    fn native_canonical_inputs_have_distinct_position_and_duration_origins() {
        for row in
            include_str!("../../../../../docs/verification/reaper-parity/beat-inputs-reference.tsv")
                .lines()
                .skip(2)
        {
            let c: Vec<_> = row.split('\t').collect();
            if c[2].contains(' ') || c[2].starts_with('-') || c[2] == "invalid" {
                continue;
            }
            let project = project(120.0, c[0].parse().unwrap(), c[1].parse().unwrap());
            for (actual, expected) in [
                (parse_position(&project, c[2]).unwrap(), c[3]),
                (parse_length(&project, c[2], 0.0).unwrap(), c[4]),
                (parse_length(&project, c[2], 72000.0).unwrap(), c[5]),
            ] {
                assert!(
                    (actual / 48000.0 - expected.parse::<f64>().unwrap()).abs() < 1e-10,
                    "{row}: {actual}"
                );
            }
        }
    }
    #[test]
    fn native_step_tempo_and_meter_duration_facts() {
        for row in include_str!(
            "../../../../../docs/verification/reaper-parity/beat-changes-reference.tsv"
        )
        .lines()
        .skip(2)
        {
            let c: Vec<_> = row.split('\t').collect();
            let mut project = project(120.0, 4, 4);
            project
                .apply(DawAction::SetTempo {
                    start_tick: 3840,
                    bpm: if c[0] == "meter" { 120.0 } else { 60.0 },
                })
                .unwrap();
            project
                .apply(DawAction::SetTimeSignature {
                    start_tick: 3840,
                    signature: TimeSignature::new(if c[0] == "tempo" { 4 } else { 3 }, 4).unwrap(),
                })
                .unwrap();
            let start = c[1].parse::<f64>().unwrap() * 48000.0;
            let length = c[2].parse::<f64>().unwrap() * 48000.0;
            assert_eq!(format_position(&project, start).unwrap(), c[3], "{row}");
            assert_eq!(
                format_length(&project, start, length).unwrap(),
                c[4],
                "{row}"
            );
            assert!(
                (parse_length(&project, c[4], start).unwrap() / 48000.0
                    - c[5].parse::<f64>().unwrap())
                .abs()
                    < 1e-10,
                "{row}"
            );
        }
    }
}
