use aaadaw_core::Project;
use aaadaw_engine::{AudioBlock, Transport, TransportPositionOverflow};

#[test]
fn transport_advances_only_while_playing_and_preserves_seek_position() {
    let mut transport = Transport::new();
    assert_eq!(
        transport
            .advance_block(128)
            .expect("stopped blocks should be valid"),
        AudioBlock {
            start_sample: 0,
            frame_count: 128,
            is_playing: false,
        }
    );
    assert_eq!(transport.position_samples(), 0);

    transport.start();
    assert_eq!(
        transport
            .advance_block(128)
            .expect("playing block should advance"),
        AudioBlock {
            start_sample: 0,
            frame_count: 128,
            is_playing: true,
        }
    );
    assert_eq!(transport.position_samples(), 128);

    transport.seek_sample(48_000);
    assert_eq!(transport.position_samples(), 48_000);
    transport.stop();
    transport
        .advance_block(256)
        .expect("stopped block should not advance the playhead");
    assert_eq!(transport.position_samples(), 48_000);
}

#[test]
fn transport_seeks_between_project_ticks_and_samples() {
    let project = Project::new();
    let mut transport = Transport::new();

    transport
        .seek_tick(&project, 960)
        .expect("valid project tick should map to a sample");

    assert_eq!(transport.position_samples(), 24_000);
    assert_eq!(
        transport
            .position_tick(&project)
            .expect("sample position should map back to a tick"),
        960
    );
}

#[test]
fn transport_overflow_does_not_corrupt_the_playhead() {
    let mut transport = Transport::new();
    transport.seek_sample(u64::MAX);
    transport.start();

    assert_eq!(transport.advance_block(1), Err(TransportPositionOverflow));
    assert_eq!(transport.position_samples(), u64::MAX);
    assert!(transport.is_playing());
}
