//! `cargo run -p keytone-core --example loadpack packs/<id>` — validate a pack.
fn main() {
    let dir = std::env::args().nth(1).expect("usage: loadpack <pack-dir>");
    let p = keytone_core::pack::LoadedPack::load(std::path::Path::new(&dir)).unwrap();
    println!("clips={} warnings={:?}", p.clip_count(), p.warnings);
    for code in ["KeyA", "Space", "Enter", "Backspace", "Tab", "ShiftLeft"] {
        let c = p.resolve(code, keytone_core::keyhook::Dir::Down);
        println!("{code:>10}: {} variant(s), {:.2}s", c.len(), c.first().map(|s| s.duration_secs()).unwrap_or(0.0));
    }
}
