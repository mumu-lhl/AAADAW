use aaadaw_engine::{MonoPcmClip, PcmError};

#[test]
fn mono_pcm_player_resamples_into_fixed_output_buffers() {
    let clip = MonoPcmClip::new(vec![0.0, 1.0], 1).expect("positive source rate is valid");
    let mut player = clip.player(2).expect("positive output rate is valid");
    let mut output = [99.0_f32; 5];

    player.render_into(&mut output);

    assert_eq!(output, [0.0, 0.5, 1.0, 1.0, 0.0]);
    assert_eq!(player.source_position(), 2.0);
}

#[test]
fn mono_pcm_player_copies_matching_rate_and_supports_seeking() {
    let clip =
        MonoPcmClip::new(vec![0.25, -0.5, 0.75], 48_000).expect("positive source rate is valid");
    let mut player = clip.player(48_000).expect("output rate should be valid");
    player
        .seek_source_frame(1)
        .expect("in-range frame should be seekable");
    let mut output = [0.0_f32; 3];

    player.render_into(&mut output);

    assert_eq!(output, [-0.5, 0.75, 0.0]);
    assert_eq!(player.source_position(), 3.0);
}

#[test]
fn empty_clips_and_invalid_rates_are_handled_without_panics() {
    let empty = MonoPcmClip::new(Vec::new(), 48_000).expect("empty clips are valid");
    let mut player = empty.player(48_000).expect("output rate should be valid");
    let mut output = [1.0_f32; 2];
    player.render_into(&mut output);
    assert_eq!(output, [0.0, 0.0]);

    assert_eq!(
        MonoPcmClip::new(vec![], 0),
        Err(PcmError::InvalidSampleRate)
    );
    let clip = MonoPcmClip::new(vec![0.0], 48_000).expect("clip should be valid");
    assert!(matches!(clip.player(0), Err(PcmError::InvalidSampleRate)));
    assert!(matches!(
        clip.player(48_000)
            .expect("player should be valid")
            .seek_source_frame(2),
        Err(PcmError::SeekOutOfRange)
    ));
}
