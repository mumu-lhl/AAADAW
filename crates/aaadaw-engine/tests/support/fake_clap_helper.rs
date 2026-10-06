use aaadaw_engine::{ClapIpcConfig, ClapIpcMapping};
use std::path::Path;
use std::thread;
use std::time::Duration;

fn main() {
    let args = std::env::args_os().collect::<Vec<_>>();
    let Some(mapping_path) = args.get(2) else {
        std::process::exit(2);
    };
    let Some(entry_path) = args.get(3) else {
        std::process::exit(2);
    };
    let Some(plugin_id) = args.get(4).and_then(|id| id.to_str()) else {
        std::process::exit(2);
    };
    let Some(sample_rate) = args.get(5).and_then(|value| value.to_str()) else {
        std::process::exit(2);
    };
    let Some(block_frames) = args.get(6).and_then(|value| value.to_str()) else {
        std::process::exit(2);
    };
    let Some(event_capacity) = args.get(7).and_then(|value| value.to_str()) else {
        std::process::exit(2);
    };
    let Some(state_output_path) = args.get(9) else {
        std::process::exit(2);
    };
    let Some(state_input_path) = args.get(8) else {
        std::process::exit(2);
    };
    let expected = ClapIpcConfig::new(
        sample_rate.parse().unwrap_or(0),
        block_frames.parse().unwrap_or(0),
        event_capacity.parse().unwrap_or(0),
    )
    .unwrap_or_else(|| std::process::exit(2));

    // SAFETY: the test supervisor owns the private, fixed-size mapping for this child lifetime.
    let mapping = unsafe { ClapIpcMapping::open(Path::new(mapping_path)) }
        .unwrap_or_else(|_| std::process::exit(3));
    let region = mapping.region();

    if plugin_id == "test.mismatch" {
        let mismatch = ClapIpcConfig::new(expected.sample_rate + 1, 16, 8)
            .unwrap_or_else(|| std::process::exit(4));
        let _ = region.accept_handshake(mismatch);
        return;
    }

    if !region.accept_handshake(expected) {
        std::process::exit(5);
    }

    match plugin_id {
        "test.crash-restart" => crash_first_launch_then_wait(
            region,
            Path::new(entry_path),
            Path::new(state_input_path),
            Path::new(state_output_path),
        ),
        "test.state-shutdown" => {
            while !region.is_shutdown() {
                thread::sleep(Duration::from_millis(1));
            }
            if write_state(Path::new(state_output_path), b"helper shutdown state").is_err() {
                std::process::exit(7);
            }
        }
        "test.stateless-shutdown" => {
            while !region.is_shutdown() {
                thread::sleep(Duration::from_millis(1));
            }
            if std::fs::write(
                state_output_path,
                [b"AAST".as_slice(), &[0], &0_u64.to_le_bytes()].concat(),
            )
            .is_err()
            {
                std::process::exit(7);
            }
        }
        "test.stall" => loop {
            thread::sleep(Duration::from_secs(1));
        },
        _ => std::process::exit(6),
    }
}

fn write_state(path: &Path, state: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, encode_state(state))
}

fn encode_state(state: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(13 + state.len());
    bytes.extend_from_slice(b"AAST");
    bytes.push(1);
    bytes.extend_from_slice(&(state.len() as u64).to_le_bytes());
    bytes.extend_from_slice(state);
    bytes
}

fn crash_while_owning_request(region: &aaadaw_engine::ClapIpcRegion) -> ! {
    loop {
        if let Some(request) = region.try_claim_request() {
            // Leave the slot in child-owned state. The supervisor must recover it after exit.
            std::mem::forget(request);
            std::process::abort();
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn crash_first_launch_then_wait(
    region: &aaadaw_engine::ClapIpcRegion,
    marker_path: &Path,
    state_input_path: &Path,
    state_output_path: &Path,
) -> ! {
    if marker_path.exists() {
        let _ = std::fs::remove_file(marker_path);
        let expected_state = encode_state(b"state before restart");
        if std::fs::read(state_input_path).ok().as_deref() != Some(expected_state.as_slice()) {
            std::process::exit(8);
        }
        wait_for_shutdown(region, state_output_path, b"state after restart")
    } else {
        if write_state(state_output_path, b"state before restart").is_err() {
            std::process::exit(7);
        }
        if std::fs::write(marker_path, b"started").is_err() {
            std::process::exit(7);
        }
        crash_while_owning_request(region)
    }
}

fn wait_for_shutdown(
    region: &aaadaw_engine::ClapIpcRegion,
    state_output_path: &Path,
    state: &[u8],
) -> ! {
    while !region.is_shutdown() {
        thread::sleep(Duration::from_millis(1));
    }
    if write_state(state_output_path, state).is_err() {
        std::process::exit(7);
    }
    std::process::exit(0)
}
