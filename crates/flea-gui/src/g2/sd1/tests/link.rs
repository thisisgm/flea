//! The ShareLink handshake and its failure steps.
use super::*;

#[test]
fn the_link_handshake_runs_sharelink_then_clipboard_then_says_so() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    let i18n = english();
    let actions = send.copy_share_link("/home/gm/file.txt", &i18n);
    assert_eq!(
        actions,
        vec![Action::Run(
            RunKind::ShareLink,
            vec![
                "dropbox-cli".into(),
                "sharelink".into(),
                "/home/gm/file.txt".into()
            ]
        )]
    );
    assert_eq!(
        send.copy_share_link("/home/gm/other.txt", &i18n),
        vec![Action::Say(
            "A share link is still being copied; try again when it finishes.".into(),
            true
        )]
    );
    let url = "https://www.dropbox.com/s/example/file.txt?dl=0";
    let next = send.sharelink_done(true, 0, url, &i18n);
    assert_eq!(
        next,
        vec![Action::Run(
            RunKind::Clipboard,
            vec!["wl-copy".into(), url.into()]
        )]
    );
    let said = send.clipboard_done(true, &i18n);
    assert_eq!(
        said,
        vec![Action::Say(
            "Share link copied to the clipboard.".into(),
            false
        )]
    );
}

#[test]
fn link_failures_name_their_step_and_a_bad_path_never_spawns() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    let i18n = english();
    assert_eq!(
        send.copy_share_link("relative.txt", &i18n),
        vec![Action::Say(
            "Flea could not find that file's location, so no share link was made.".into(),
            true
        )]
    );
    assert!(send
        .copy_share_link("/home/gm/file.txt", &i18n)
        .iter()
        .any(|action| matches!(action, Action::Run(_, _))));
    assert_eq!(
        send.sharelink_done(false, 0, "", &i18n),
        vec![Action::Say(
            "Dropbox could not be asked for a share link.".into(),
            true
        )]
    );
    assert!(send
        .copy_share_link("/home/gm/file.txt", &i18n)
        .iter()
        .any(|action| matches!(action, Action::Run(_, _))));
    assert_eq!(
        send.sharelink_done(true, 0, "Dropbox isn't running!", &i18n),
        vec![Action::Say(
            "Dropbox could not make a share link for that file.".into(),
            true
        )]
    );
}
