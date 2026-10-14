//! Headless playback smoke test: streams a real station for a few seconds,
//! reports player events, and checks the spectrum bands react to live audio.

use std::time::{Duration, Instant};

fn main() {
    // Radio Paradise Main Mix (EU) 320k AAC — a reliable, long-running stream.
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "http://stream-uk1.radioparadise.com/aac-320".to_string());

    let player = bwr_player::Player::new();
    player.set_volume(0.0); // silence the speakers during the test
    player.play(&url);

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut got_playing = false;
    let mut max_band: f32 = 0.0;
    let mut snapshots = 0u32;

    while Instant::now() < deadline {
        let gen = player.generation();
        for (event_gen, event) in player.poll() {
            if event_gen != gen {
                continue;
            }
            println!("event: {event:?}");
            if matches!(event, bwr_player::PlayerEvent::Playing) {
                got_playing = true;
            }
        }
        if got_playing {
            player.spectrum.update();
            let mut bands = [0.0f32; bwr_spectrum::BANDS];
            player.spectrum.snapshot(&mut bands);
            snapshots += 1;
            max_band = max_band.max(bands.iter().copied().fold(0.0f32, f32::max));
            if snapshots >= 20 {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    player.stop();

    println!("got_playing={got_playing} snapshots={snapshots} max_band={max_band:.3}");
    if got_playing && snapshots >= 20 && max_band > 0.01 {
        println!("PLAYBACK OK");
    } else {
        println!("PLAYBACK FAILED");
        std::process::exit(1);
    }
}
