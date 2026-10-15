use std::time::Instant;

fn main() {
    let client = bwr_core::RadioBrowserClient::new().expect("client init");

    let t = Instant::now();
    match client.random_station(128, "") {
        Ok(s) => println!("random_station(128): {:?} -> {} [{} kbps {}]", t.elapsed(), s.name, s.bitrate, s.codec),
        Err(e) => println!("random_station(128) FAILED after {:?}: {e}", t.elapsed()),
    }

    let t = Instant::now();
    match client.random_station(0, "") {
        Ok(s) => println!("random_station(0): {:?} -> {} [{} kbps {}]", t.elapsed(), s.name, s.bitrate, s.codec),
        Err(e) => println!("random_station(0) FAILED after {:?}: {e}", t.elapsed()),
    }

    let t = Instant::now();
    match client.similar("jazz", "nonexistent-uuid", 0, "") {
        Ok(v) => println!("similar(jazz): {:?} -> {} stations", t.elapsed(), v.len()),
        Err(e) => println!("similar(jazz) FAILED after {:?}: {e}", t.elapsed()),
    }
}
