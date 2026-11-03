//! Mountinfo injection reaches the mounted window and the same rail rebuild as the live poll.
use super::*;
use crate::ipc::server::{Answer, Event};
use flea::ipc::wire::{Reply, Request};
use gpui::{px, size};

fn send(
    shell: &Entity<Shell>,
    target: &str,
    function: &str,
    arguments: &[&str],
    cx: &mut VisualTestContext,
) -> std::sync::mpsc::Receiver<Answer> {
    let (answer, receive) = std::sync::mpsc::sync_channel::<Answer>(1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: target.into(),
                        function: function.into(),
                        arguments: arguments
                            .iter()
                            .map(|argument| (*argument).into())
                            .collect(),
                    },
                    answer,
                },
                window,
                cx,
            )
        })
    });
    receive
}

fn call(
    shell: &Entity<Shell>,
    target: &str,
    function: &str,
    arguments: &[&str],
    cx: &mut VisualTestContext,
) -> Reply {
    let receive = send(shell, target, function, arguments, cx);
    cx.run_until_parked();
    receive
        .try_recv()
        .expect("the mounted IPC call answered")
        .reply
}

fn root(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(crate::g2::pl1::source::fixture::Services {
            runner: crate::window::network::default_runner(),
            hostname: "testhost".into(),
        })
    });
    let (backend, _) = Backend::test_lines();
    let (root, cx) = cx.add_window_view(|window, cx| crate::window::root::Root {
        shell: cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            window.focus(&shell.focus, cx);
            let join = mount(
                shell.locale.catalog.clone(),
                &shell.launch.settings.data,
                0,
                cx,
            );
            shell.slots.rail = Some(AnyView::from(join.host.clone()));
            shell.rail = Some(join);
            shell
        }),
    });
    let shell = root.read_with(cx, |root, _| root.shell.clone());
    cx.simulate_resize(size(px(1100.), px(700.)));
    frame(cx);
    (shell, cx)
}

#[gpui::test]
fn pb_network_mountinfo_rebuilds_root_rows_without_replacing_gio(cx: &mut TestAppContext) {
    let (shell, cx) = root(cx);
    cx.simulate_keystrokes("a");
    frame(cx);
    assert_eq!(
        call(&shell, "flea", "networkFocus", &[], cx),
        Reply::value("Host")
    );
    shell.update(cx, |shell, cx| {
        shell
            .rail
            .as_ref()
            .unwrap()
            .devices
            .update(cx, |source, _| {
                source.fixture_mount_listing().text =
                    "Mount(0): Saved share -> smb://nas/share\n".into();
            });
        let network = shell.network().unwrap();
        network.listing.text = "Mount(0): Saved share -> smb://nas/share\n".into();
    });
    let text = "30 30 0:30 / /fixture/nas rw - cifs //nas/share rw\n\
                31 31 0:31 / /fixture/nfs\\040share rw - nfs4 nas:/export rw\n\
                32 32 0:32 / /fixture/local rw - ext4 /dev/sda rw";
    assert_eq!(
        call(&shell, "flea", "networkFeedMountinfo", &[text], cx),
        Reply::value("fed")
    );
    frame(cx);
    assert_eq!(
        call(&shell, "flea", "networkEntries", &[], cx),
        Reply::value(
            "Saved share|network|share|true\nnas|network|share|true\nnfs share|network|share|true"
        )
    );
    assert_eq!(
        call(&shell, "flea", "networkBuilt", &[], cx),
        Reply::value("false"),
        "injection does not claim the first real listing finished"
    );
    assert_eq!(
        call(&shell, "flea", "networkFeedMountinfo", &[""], cx),
        Reply::value("fed")
    );
    frame(cx);
    assert_eq!(
        call(&shell, "flea", "networkEntries", &[], cx),
        Reply::value("Saved share|network|share|true"),
        "an empty mountinfo snapshot removes injected kernel rows, retaining gio"
    );
}

#[gpui::test]
fn sol_gh374_375_mac_cifs_children_do_not_flood_the_mounted_rail(cx: &mut TestAppContext) {
    let (shell, cx) = root(cx);
    let fixture = include_str!("../../../../src/ui_logic_pl2/fixtures/gh374.mountinfo");
    assert_eq!(
        call(&shell, "flea", "networkFeedMountinfo", &[fixture], cx),
        Reply::value("fed")
    );
    frame(cx);
    assert_eq!(
        call(&shell, "flea", "networkEntries", &[], cx),
        Reply::value("MacMini|network|share|true\nMediaDrive|network|share|true"),
        "23 iCloud Documents submounts belong to MacMini, while MediaDrive stays independent"
    );
}

// The scheduler orders the two background reads by seed, so the sweep covers seeds where the older read finishes last.
#[gpui::test(iterations = 16)]
fn pb_network_mountinfo_newer_snapshot_supersedes_older_read(cx: &mut TestAppContext) {
    let (shell, cx) = root(cx);
    let old = send(
        &shell,
        "flea",
        "networkFeedMountinfo",
        &["30 30 0:30 / /fixture/older rw - cifs //nas/older rw"],
        cx,
    );
    let new = send(
        &shell,
        "flea",
        "networkFeedMountinfo",
        &["31 31 0:31 / /fixture/newer rw - nfs4 nas:/newer rw"],
        cx,
    );
    cx.run_until_parked();
    for receive in [old, new] {
        assert_eq!(receive.try_recv().unwrap().reply, Reply::value("fed"));
    }
    frame(cx);
    assert_eq!(
        call(&shell, "flea", "networkEntries", &[], cx),
        Reply::value("newer|network|share|true")
    );
}

#[gpui::test]
fn pb_network_mountinfo_validates_calls_and_does_not_construct_a_service(cx: &mut TestAppContext) {
    let (shell, cx) = crate::window::u07_tests::open(cx);
    assert_eq!(
        call(&shell, "flea", "networkFeedMountinfo", &[""], cx),
        Reply::value("no service")
    );
    for arguments in [vec![], vec!["", "extra"]] {
        let reply = call(&shell, "flea", "networkFeedMountinfo", &arguments, cx);
        assert!(
            reply.stdout.contains("arguments provided"),
            "the registry still validates argument counts: {reply:?}"
        );
    }
    assert_eq!(
        call(&shell, "fleapicker", "networkFeedMountinfo", &[""], cx),
        Reply::value("Function not found."),
        "only the main-window target owns the seam"
    );
    assert!(shell.read_with(cx, |shell, _| shell.rail.is_none()));
}
