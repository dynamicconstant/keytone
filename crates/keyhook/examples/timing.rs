//! Prints the arrival time of each key event (diagnostics).
fn main() {
    let t0 = std::time::Instant::now();
    keyhook::start(move |ev| println!("{:>7.1} ms  {:>10} {:?}", t0.elapsed().as_secs_f64() * 1000.0, ev.code, ev.dir)).unwrap();
    loop { std::thread::park(); }
}
