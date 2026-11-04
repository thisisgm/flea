// The host broker runs this standalone launcher in its pinned checkout.
use std::os::unix::process::CommandExt;
fn main() {
    let error = std::process::Command::new("python3")
        .arg("-B")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/campaign.py"))
        .args(std::env::args().skip(1))
        .exec();
    eprintln!("search instrument: {error}");
    std::process::exit(2);
}
