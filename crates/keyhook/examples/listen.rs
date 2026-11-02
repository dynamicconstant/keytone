//! `cargo run -p keyhook --example listen` — prints key codes as you type.
fn main() {
    println!("permission: {:?}", keyhook::permission());
    keyhook::start(|ev| println!("{:>14}  {:?}", ev.code, ev.dir)).expect("hook failed");
    println!("listening — press keys (Ctrl+C to quit)");
    loop {
        std::thread::park();
    }
}
