//! Issue #374: mount ancestry and source paths distinguish CIFS children from separate shares.
use super::rail::rebuild;

const MAC: &str = include_str!("fixtures/gh374.mountinfo");

fn paths(info: &str) -> Vec<String> {
    rebuild("", &[], info)
        .into_iter()
        .map(|row| row.path)
        .collect()
}

#[test]
fn sol_gh374_375_mac_children_collapse_without_reordering_independent_shares() {
    assert_eq!(paths(MAC), ["/fixture/MacMini", "/fixture/MediaDrive"]);
    let reversed = MAC.lines().rev().collect::<Vec<_>>().join("\n");
    assert_eq!(
        paths(&reversed),
        ["/fixture/MediaDrive", "/fixture/MacMini"]
    );
}

#[test]
fn sol_gh374_375_nested_independent_sources_and_sibling_mounts_stay() {
    let info = format!(
        "{MAC}\
201 100 0:81 / /fixture/MacMini/Separate rw - cifs //macmini/Separate rw\n\
202 100 0:82 / /fixture/MacMini/OtherHost rw - cifs //other/Macintosh\\040HD/OtherHost rw\n\
203 1 0:83 / /fixture/OtherMac rw - cifs //macmini/Macintosh\\040HD rw\n\
204 100 0:84 / /fixture/MacMini/Export rw - nfs4 nas:/export rw\n"
    );
    assert_eq!(
        paths(&info),
        [
            "/fixture/MacMini",
            "/fixture/MediaDrive",
            "/fixture/MacMini/Separate",
            "/fixture/MacMini/OtherHost",
            "/fixture/OtherMac",
            "/fixture/MacMini/Export",
        ]
    );
}

#[test]
fn sol_gh374_375_missing_ancestry_and_prefix_collisions_never_hide_shares() {
    let info = "100 1 0:40 / /fixture/Mac rw - cifs //mac/share rw\n\
101 999 0:41 / /fixture/Mac/Orphan rw - cifs //mac/share/Orphan rw\n\
102 100 0:42 / /fixture/Macintosh/Docs rw - cifs //mac/share/Docs rw\n\
103 100 0:43 / /fixture/Mac/Other rw - cifs //mac/share2/Other rw\n";
    assert_eq!(paths(info).len(), 4);
}

#[test]
fn sol_gh374_375_cifs_subroots_and_intermediate_mounts_use_real_ancestry() {
    let info = "100 1 0:40 / /fixture/Mac rw - cifs //mac/share rw\n\
101 100 0:41 /dir /fixture/Mac/dir rw - cifs //mac/share rw\n\
102 101 0:42 / /fixture/Mac/dir/local rw - tmpfs tmpfs rw\n\
103 102 0:43 / /fixture/Mac/dir/local/Docs rw - cifs //mac/share/dir/local/Docs rw\n";
    assert_eq!(paths(info), ["/fixture/Mac"]);
}

#[test]
fn sol_gh374_375_kernel_gio_labels_survive_and_child_file_rows_do_not_duplicate() {
    let gio = "Mount(0): My Mac -> file:///fixture/MacMini\n\
Mount(1): Documents -> file:///fixture/MacMini/System/Volumes/Data/Users/test/Library/Mobile%20Documents/com~apple~app0/Documents\n";
    let rows = rebuild(gio, &[], MAC);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].label, "My Mac");
    assert_eq!(rows[0].uri, "file:///fixture/MacMini");
}

#[test]
fn sol_gh374_375_cycles_and_stacked_mounts_are_bounded_and_keep_latest_path() {
    let info = "100 101 0:40 / /fixture/Mac rw - cifs //mac/share rw\n\
101 100 0:41 / /fixture/Other rw - cifs //other/share rw\n\
102 1 0:42 / /fixture/Mac rw - smb3 //new/share rw\n";
    let rows = rebuild("", &[], info);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].fstype, "smb3");
}

#[test]
fn sol_gh374_375_subroot_mounts_compare_effective_remote_paths() {
    for (source, child, count) in [
        ("//mac/share", "//mac/share/sub/Docs", 1),
        ("//mac/share", "//mac/share/Docs", 2),
        ("//mac/share/sub", "//mac/share/sub/Docs", 1),
    ] {
        let info = format!(
            "100 1 0:40 /sub /fixture/Mac rw - cifs {source} rw\n\
101 100 0:41 / /fixture/Mac/Docs rw - cifs {child} rw\n"
        );
        assert_eq!(paths(&info).len(), count, "{source} -> {child}");
    }
}
