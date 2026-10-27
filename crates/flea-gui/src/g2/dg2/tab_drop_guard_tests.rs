//! Pending tab drops retain their target identity through disk-fact latency.
use super::*;
use crate::g2::dg2::{mounted_tests as mounted, pb_devices_2_tests};

#[test]
fn pb_devices_2_tab_destination_follows_identity_and_rejects_closed_or_changed_folders() {
    let root = mounted::Root::new("tab-stale");
    let dest = format!("{}/alpha", root.src());
    let mut cx = crate::list::bench::real_cx();
    let (handle, _) = mounted::open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, _, _| {
            pb_devices_2_tests::tabs(shell, &dest)
        })
        .unwrap();
    mounted::frame(&mut cx, handle);
    handle
        .update(&mut cx, |shell, _, _| {
            let tab = Destination {
                side: 0,
                identity: 2,
                path: dest.clone(),
            };
            assert!(shell.file_tab_ready(&tab));
            shell.strip.tabs[0].tabs.as_mut().unwrap().items.swap(0, 1);
            shell.strip.tabs[0].tabs.as_mut().unwrap().index = 1;
            assert!(
                shell.file_tab_ready(&tab),
                "a reordered tab retains its identity"
            );
            shell.strip.tabs[0].tabs.as_mut().unwrap().items[0].path = "/different".into();
            assert!(
                !shell.file_tab_ready(&tab),
                "a new folder invalidates the pending facts"
            );
            shell.strip.tabs[0].tabs.as_mut().unwrap().items.remove(0);
            assert!(
                !shell.file_tab_ready(&tab),
                "a closed tab never redirects the drop"
            );
        })
        .unwrap();
}
