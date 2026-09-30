use aaadaw_core::{DawAction, Project};
use aaadaw_engine::{MixError, MixerPlan};

fn project_with_tracks(names: &[&str]) -> (Project, Vec<aaadaw_core::TrackId>) {
    let mut project = Project::new();
    for (index, name) in names.iter().enumerate() {
        project
            .apply(DawAction::CreateTrack {
                index,
                name: (*name).to_owned(),
            })
            .expect("track creation should succeed");
    }
    let track_ids = project.tracks().iter().map(|track| track.id()).collect();
    (project, track_ids)
}

#[test]
fn mixer_applies_equal_power_center_pan_and_track_gain() {
    let (mut project, track_ids) = project_with_tracks(&["Center"]);
    project
        .apply(DawAction::SetTrackVolume {
            track_id: track_ids[0],
            volume_db: -6.0,
        })
        .expect("track volume should be accepted");
    let plan = MixerPlan::compile(project.tracks(), 8).expect("track controls should compile");
    let input = [1.0_f32, -0.5];
    let mut output = [[0.0_f32; 2]; 2];

    plan.mix_mono_into(&[&input], &mut output)
        .expect("matching buffers should mix");

    let expected = 10.0_f32.powf(-6.0 / 20.0) * std::f32::consts::FRAC_1_SQRT_2;
    assert!((output[0][0] - expected).abs() < 1.0e-6);
    assert!((output[0][1] - expected).abs() < 1.0e-6);
    assert!((output[1][0] + expected * 0.5).abs() < 1.0e-6);
    assert!((output[1][1] + expected * 0.5).abs() < 1.0e-6);
}

#[test]
fn mixer_obeys_mute_solo_and_hard_pan_controls() {
    let (mut project, track_ids) = project_with_tracks(&["Muted", "Solo", "Other"]);
    project
        .apply(DawAction::SetTrackMute {
            track_id: track_ids[0],
            muted: true,
        })
        .expect("track mute should be accepted");
    project
        .apply(DawAction::SetTrackSolo {
            track_id: track_ids[1],
            solo: true,
        })
        .expect("track solo should be accepted");
    let plan = MixerPlan::compile(project.tracks(), 8).expect("track controls should compile");
    let inputs: [&[f32]; 3] = [&[10.0], &[0.5], &[20.0]];
    let mut output = [[0.0_f32; 2]; 1];

    plan.mix_mono_into(&inputs, &mut output)
        .expect("matching buffers should mix");

    let expected = 0.5 * std::f32::consts::FRAC_1_SQRT_2;
    assert!((output[0][0] - expected).abs() < 1.0e-6);
    assert!((output[0][1] - expected).abs() < 1.0e-6);

    let (mut project, track_ids) = project_with_tracks(&["Left", "Right"]);
    project
        .apply(DawAction::SetTrackPan {
            track_id: track_ids[0],
            pan: -1.0,
        })
        .expect("left pan should be accepted");
    project
        .apply(DawAction::SetTrackPan {
            track_id: track_ids[1],
            pan: 1.0,
        })
        .expect("right pan should be accepted");
    let plan = MixerPlan::compile(project.tracks(), 8).expect("track controls should compile");
    let inputs: [&[f32]; 2] = [&[0.25], &[0.75]];
    plan.mix_mono_into(&inputs, &mut output)
        .expect("matching buffers should mix");
    assert!((output[0][0] - 0.25).abs() < 1.0e-6);
    assert!((output[0][1] - 0.75).abs() < 1.0e-6);
}

#[test]
fn invalid_callback_buffers_leave_output_untouched() {
    let (project, _) = project_with_tracks(&["Track"]);
    let plan = MixerPlan::compile(project.tracks(), 1).expect("track controls should compile");
    let mut output = [[9.0_f32, -9.0_f32]; 2];

    let result = plan.mix_mono_into(&[&[1.0, 2.0]], &mut output);

    assert_eq!(
        result,
        Err(MixError::BlockTooLarge {
            requested: 2,
            maximum: 1,
        })
    );
    assert_eq!(output, [[9.0, -9.0]; 2]);
}
