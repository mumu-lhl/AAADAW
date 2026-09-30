use aaadaw_engine::{PcmStreamError, pcm_stream};

#[test]
fn spsc_pcm_stream_preserves_order_and_reports_underflow() {
    let (mut producer, mut consumer) = pcm_stream(3).expect("positive capacity is valid");
    assert_eq!(producer.push_samples(&[1.0, 2.0, 3.0, 4.0]), 3);
    assert_eq!(producer.available_capacity(), 0);
    assert_eq!(consumer.available_samples(), 3);

    let mut first_block = [0.0_f32; 2];
    assert_eq!(consumer.read_into(&mut first_block), 0);
    assert_eq!(first_block, [1.0, 2.0]);
    assert_eq!(producer.push_samples(&[4.0, 5.0]), 2);

    let mut second_block = [0.0_f32; 4];
    assert_eq!(consumer.read_into(&mut second_block), 1);
    assert_eq!(second_block, [3.0, 4.0, 5.0, 0.0]);
}

#[test]
fn empty_queue_fills_the_entire_audio_block_with_silence() {
    let (mut producer, mut consumer) = pcm_stream(2).expect("positive capacity is valid");
    producer.push_samples(&[0.25]);
    let mut output = [7.0_f32; 3];

    assert_eq!(consumer.read_into(&mut output), 2);
    assert_eq!(output, [0.25, 0.0, 0.0]);
    assert_eq!(consumer.read_into(&mut []), 0);
}

#[test]
fn zero_capacity_is_rejected() {
    assert!(matches!(pcm_stream(0), Err(PcmStreamError::ZeroCapacity)));
}
