use std::time::Instant;

fn main() {
    let t0 = Instant::now();
    let client = bwr_core::RadioBrowserClient::new().expect("client init");
    println!("new(): {:?}", t0.elapsed());

    let t1 = Instant::now();
    let stations = client.trending(256, "").expect("trending");
    println!("trending(): {:?} ({} stations)", t1.elapsed(), stations.len());

    let t2 = Instant::now();
    let stations = client.trending(256, "").expect("trending2");
    println!("trending2(): {:?} ({} stations)", t2.elapsed(), stations.len());
}
