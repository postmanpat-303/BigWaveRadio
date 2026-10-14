//! Rapid station-switch test: cycles through several streams quickly, the way
//! a user clicking around the grid does, then verifies the final stream
//! reaches Playing. Run: cargo run --release -p bwr-player --example switchtest

use std::time::{Duration, Instant};

fn main() {
    let urls = [
        "http://stream-uk1.radioparadise.com/aac-320",
        "http://ice1.somafm.com/groovesalad-128-mp3",
        "http://ice1.somafm.com/dronezone-128-mp3",
        "http://stream-uk1.radioparadise.com/aac-320",
    ];

    let player = bwr_player::Player::new();
    player.set_volume(0.0);

    let t0 = Instant::now();
    for url in &urls {
        player.play(url);
        std::thread::sleep(Duration::from_millis(1200)); // faster than a human, deliberately
    }

    // The final station must reach Playing within 20 s of the first click.
    let deadline = t0 + Duration::from_secs(20) + Duration::from_millis(1200 * urls.len() as u64);
    let mut got_playing = false;
    let mut events = 0u32;
    while Instant::now() < deadline && !got_playing {
        let gen = player.generation();
        for (g, ev) in player.poll() {
            if g != gen {
                continue;
            }
            events += 1;
            println!("current-gen event: {ev:?}");
            if matches!(ev, bwr_player::PlayerEvent::Playing) {
                got_playing = true;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    player.stop();

    println!("switches={} events_seen={events} final_playing={got_playing}", urls.len());
    if got_playing {
        println!("SWITCH TEST OK");
    } else {
        println!("SWITCH TEST FAILED");
        std::process::exit(1);
    }
}
