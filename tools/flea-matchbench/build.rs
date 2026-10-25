// Copies the resolved lock into OUT_DIR so the binary can print the exact dependency set it was built from.
fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir");
    let out = std::env::var("OUT_DIR").expect("out dir");
    let lock = std::path::Path::new(&manifest).join("Cargo.lock");
    let text = std::fs::read_to_string(&lock).unwrap_or_default();
    std::fs::write(std::path::Path::new(&out).join("resolved.lock"), text)
        .expect("write lock copy");
    println!("cargo:rerun-if-changed=Cargo.lock");
}
