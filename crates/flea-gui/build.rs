use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("crate directory"))
        .join("../../ci/visual/scenes");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut paths: Vec<_> = fs::read_dir(root)
        .expect("scene manifest directory")
        .map(|entry| entry.expect("scene manifest entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "scene registry is empty");
    let entries = paths
        .iter()
        .map(|path| {
            format!(
                "include_str!({:?}),",
                path.to_str().expect("UTF-8 scene path")
            )
        })
        .collect::<String>();
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("build output directory"));
    fs::write(output.join("visual-scenes.rs"), format!("&[{entries}]"))
        .expect("compiled scene registry");
    startup_order(&output)?;
    Ok(())
}

// The functions a launch runs, laid out together at the front of the code, so a launch reads and maps
// about 8 MB of code rather than about 23; see AGENTS.md "The first window".
fn startup_order(output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let list = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("missing crate directory")?)
        .join("startup-order.txt");
    println!("cargo:rerun-if-changed={}", list.display());
    let linux_x86 = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux")
        && env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("x86_64");
    // Only the release layout was measured; a debug link would pay for the pattern matching and gain nothing.
    if !linux_x86 || env::var("PROFILE").as_deref() != Ok("release") {
        return Ok(());
    }
    let names = fs::read_to_string(&list)
        .map_err(|error| format!("startup order list {}: {error}", list.display()))?;
    let mut script = String::from("SECTIONS\n{\n  .text.flea.startup :\n  {\n");
    // LLVM files a cold function, such as a vector's grow path, under .text.unlikely, and launches run those too.
    for name in names
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        script.push_str(&format!("    *(.text.{name} .text.unlikely.{name})\n"));
    }
    script.push_str("  }\n}\nINSERT BEFORE .text;\n");
    let path = output.join("startup-order.ld");
    fs::write(&path, script)
        .map_err(|error| format!("startup order script {}: {error}", path.display()))?;
    let target = env::var("TARGET")?;
    if links_with(&path, output, &target) {
        println!(
            "cargo:rustc-link-arg-bin=flea-gui=-Wl,-T,{}",
            path.display()
        );
    } else {
        println!("cargo:warning=flea-gui: this linker refused the startup order script, so the code keeps its default layout");
    }
    Ok(())
}

// mold and other linkers without SECTIONS support would fail the real link, so a one-line program is linked first.
fn links_with(script: &Path, output: &Path, target: &str) -> bool {
    let mut command = Command::new(env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
    command
        .args([
            "--crate-name",
            "startup_order_probe",
            "--crate-type",
            "bin",
            "--edition",
            "2021",
            "-o",
        ])
        .arg(output.join("startup-order-probe"))
        .arg(format!("--target={target}"))
        .arg(format!("-Clink-arg=-Wl,-T,{}", script.display()));
    if let Ok(linker) = env::var("RUSTC_LINKER") {
        command.arg(format!("-Clinker={linker}"));
    }
    if let Ok(flags) = env::var("CARGO_ENCODED_RUSTFLAGS") {
        command.args(flags.split('\x1f').filter(|flag| !flag.is_empty()));
    }
    let Ok(mut child) = command
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let fed = child
        .stdin
        .take()
        .is_some_and(|mut stdin| stdin.write_all(b"fn main() {}\n").is_ok());
    child.wait().is_ok_and(|status| status.success()) && fed
}
