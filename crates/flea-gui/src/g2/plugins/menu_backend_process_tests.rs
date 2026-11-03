use super::*;

#[test]
fn plugin_host_helper_is_private_to_each_test_process() {
    const CHILD: &str = "FLEA_PLUGIN_HELPER_CHILD";
    if std::env::var_os(CHILD).is_some() {
        println!("\nPLUGIN_HOST_HELPER={}", binary().display());
        return;
    }
    let local = binary().to_path_buf();
    let module = module_path!().strip_prefix("flea_gui::").unwrap();
    let test = format!("{module}::plugin_host_helper_is_private_to_each_test_process");
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &test, "--nocapture"])
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "helper child: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Sample input: PLUGIN_HOST_HELPER=/build/target/debug/deps/flea-plugin-host-1234
    let child = stdout
        .lines()
        .find_map(|line| line.strip_prefix("PLUGIN_HOST_HELPER="))
        .expect("child test ran and reported its real helper executable");
    assert_ne!(
        local,
        PathBuf::from(child),
        "test processes must not overwrite each other's host"
    );
    assert_eq!(binary(), local, "one process keeps its cached helper");
}
