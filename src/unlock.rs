use crate::gui;
use crate::paths;

// flea --unlock <device>: opens the unlock dialog a locked rail row opens; the passphrase is never argv.
pub const ENV: &str = "FLEA_UNLOCK";

// The exit code when argv names --unlock, or None to keep dispatching; one line in main() reaches it.
pub fn dispatch(args: &[String]) -> Option<i32> {
    if args.get(1).map(String::as_str) != Some("--unlock") {
        return None;
    }
    Some(if args.len() == 3 {
        run(&args[2])
    } else {
        crate::usage("--unlock takes one device path under /dev")
    })
}

// Sample input: "/dev/sdb1", the LUKS container to unlock.
pub fn validate(device: &str) -> Result<String, String> {
    // A dot-dot part would leave /dev, so it is refused however the path starts.
    if device.starts_with("/dev/") && device.len() > 5 && !device.contains('\n')
        && !device.split('/').any(|part| part == "..") {
        return Ok(device.to_string());
    }
    Err("flea: --unlock takes one device path under /dev".to_string())
}

pub fn run(device: &str) -> i32 {
    let target = match validate(device) {
        Ok(d) => d,
        Err(message) => {
            eprintln!("{}", message);
            return 2;
        }
    };
    if gui::reject_legacy_frontend() {
        return 2;
    }
    if !paths::has_display() {
        eprintln!("flea: there is no graphical session to open a window in");
        return 2;
    }
    gui::exec_gpui_unlock(&target)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_device_node_under_dev_is_taken() {
        assert_eq!(validate("/dev/sdb1"), Ok("/dev/sdb1".to_string()));
        assert_eq!(validate("/dev/mapper/luks-vault"), Ok("/dev/mapper/luks-vault".to_string()));
    }

    #[test]
    fn anything_else_names_the_shape() {
        for bad in ["", "/etc/passwd", "sdb1", "dev/sdb1", "/dev/", "/dev/sdb1\n", "/dev/../etc/passwd",
                    "/dev/sda/../../etc/passwd", "/dev/mapper/../sda"] {
            let message = validate(bad).expect_err("the device must be refused");
            assert!(message.contains("--unlock"), "{} should name {}, got {}", bad, "--unlock", message);
        }
    }
}
