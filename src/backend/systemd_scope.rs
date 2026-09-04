use crate::backend::cgroup::LIMITS;

const SYSTEMD_RUN: &str = "systemd-run";

pub fn transient(inner: &[String]) -> Vec<String> {
    transient_with(inner, LIMITS)
}

pub fn transient_with(inner: &[String], limits: crate::backend::cgroup::Limits) -> Vec<String> {
    scope(&[format!("MemoryHigh={}", limits.high), format!("MemoryMax={}", limits.max), format!("MemorySwapMax={}", limits.swap_max)], inner)
}

pub fn delegated(inner: &[String]) -> Vec<String> {
    scope(&["Delegate=memory".to_string()], inner)
}

fn scope(properties: &[String], inner: &[String]) -> Vec<String> {
    let mut argv = vec![SYSTEMD_RUN.to_string(), "--user".to_string(), "--scope".to_string(), "--quiet".to_string(), "--collect".to_string(), "--expand-environment=no".to_string()];
    for property in properties {
        argv.push(format!("--property={property}"));
    }
    argv.extend_from_slice(inner);
    argv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_scopes_derive_the_fixed_production_limits() {
        let got = transient(&["program".to_string()]);
        let expected = crate::backend::cgroup::Limits { high: 768 * 1024 * 1024, max: 1024 * 1024 * 1024, swap_max: 0 };
        assert_eq!((LIMITS.high, LIMITS.max, LIMITS.swap_max), (expected.high, expected.max, expected.swap_max));
        assert!(got.contains(&format!("--property=MemoryHigh={}", LIMITS.high)));
        assert!(got.contains(&format!("--property=MemoryMax={}", LIMITS.max)));
        assert!(got.contains(&format!("--property=MemorySwapMax={}", LIMITS.swap_max)));
        assert!(!got.iter().any(|arg| arg.contains("MemoryOOMGroup")));
    }

    #[test]
    fn delegated_and_transient_scopes_share_the_same_safe_prefix() {
        let transient = transient(&["transient".to_string()]);
        let delegated = delegated(&["delegated".to_string()]);
        assert_eq!(&transient[..6], &delegated[..6]);
        assert!(delegated.contains(&"--property=Delegate=memory".to_string()));
    }
}
