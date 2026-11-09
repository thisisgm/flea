use super::download_dir;
use crate::{backend::Backend, state::tests::shell_with};
use flea::ui_logic_send_share::airdrop::{Adapter, Readiness};
use gpui::{TestAppContext, VisualTestContext};

#[test]
fn received_files_go_to_the_last_download_dir_named_else_downloads() {
    let body = "XDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_DOWNLOAD_DIR=\"$HOME/Inbox\"\nXDG_DOWNLOAD_DIR=\"/mnt/nas/in\"\n";
    assert_eq!(download_dir(body, "/home/u"), "/mnt/nas/in");
    assert_eq!(
        download_dir("XDG_DOWNLOAD_DIR=\"$HOME/Inbox\"", "/home/u"),
        "/home/u/Inbox"
    );
    assert_eq!(
        download_dir(
            "XDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_DOWNLOAD_DIR=relative",
            "/home/u"
        ),
        "/home/u/Downloads",
        "an unquoted or relative value is not a folder"
    );
    assert_eq!(download_dir("", "/home/u"), "/home/u/Downloads");
}

#[gpui::test]
fn a_radio_that_cannot_start_says_which_program_and_why(cx: &mut TestAppContext) {
    // Up to 5 s for the failed start to land as an error.
    const POLL_STEPS: usize = 500;
    const POLL_MS: u64 = 10;
    const MISSING: &str = "/nonexistent/flea-sd1/owly";
    cx.executor().allow_parking();
    let (backend, _lines) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    let errors = |cx: &mut VisualTestContext| {
        shell.update(cx, |shell, cx| {
            shell.status_messages(cx, |messages, _| {
                messages
                    .errors()
                    .iter()
                    .map(|e| e.text.clone())
                    .collect::<Vec<_>>()
            })
        })
    };
    shell.update_in(cx, |shell, window, cx| {
        let air = &mut shell.menu.providers.airdrop;
        air.facts(MISSING);
        air.adapter = Adapter {
            readiness: Readiness::Ready,
            card: String::new(),
        };
        let acts = air.turn_on("/tmp", &shell.locale.catalog);
        shell.airdrop_acts(0, acts, window, cx);
    });
    for _ in 0..POLL_STEPS {
        cx.run_until_parked();
        if !errors(cx).is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
    }
    let said = errors(cx);
    assert!(
        said.iter()
            .any(|e| e.contains(MISSING) && e.contains("No such file")),
        "{said:?}"
    );
}
