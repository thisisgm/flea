//! A single marked directory keeps directory navigation even when its name looks like an archive.
use super::{answering, launched, looking, quit, shell};
use crate::{
    backend::Event,
    window::dispatch::test_shell::{asked, land_on, row, set_cursor},
};
use flea::ui_logic_op3::runfile::IS_DIRECTORY;
use gpui::TestAppContext;

fn marked_archive_named_directory(cx: &mut TestAppContext, offscreen: bool) {
    let (shell, cx, _, _keep) = shell(cx);
    cx.simulate_keystrokes("f5");
    let first = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.first_count()
    });
    let (selected, total, rows) = if offscreen {
        (
            900,
            1000,
            (0..first)
                .map(|i| row(&format!("file-{i}.txt"), false))
                .collect(),
        )
    } else {
        (
            0,
            2,
            vec![row("backup.tar.zst", true), row("notes.txt", false)],
        )
    };
    land_on(&shell, cx, 0, "/files", rows, total, 2);
    assert!(shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        !host.pane.busy()
            && host.pane.held.listing == 2
            && host.pane.held.row(selected).is_none() == offscreen
    }));
    set_cursor(&shell, cx, 1);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![selected]
    });
    // The real opener's directory status is the seam between file classification and navigation.
    answering(Some(IS_DIRECTORY), "");
    cx.simulate_keystrokes("enter");
    let id = shell.read_with(cx, |shell, _| shell.menu.sequence);
    for line in [
        format!(r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true,"kinds":[16877]}}"#),
        format!(
            r#"{{"t":"menuaction","op":"validate","action":"open","id":{id},"ok":true,"paths":["/files/backup.tar.zst"]}}"#
        ),
    ] {
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.receive(0, Event::Producer(line), window, cx)
            })
        });
        cx.run_until_parked();
    }
    assert_eq!(launched(), ["/files/backup.tar.zst"], "a captured directory must reach the existing single-path opener instead of archive preview");
    assert_eq!(asked(&shell, cx), "/files/backup.tar.zst");
    assert_eq!(looking(&shell, cx), "false");
    assert!(!shell.read_with(cx, |shell, _| shell.open_selection_parked(0)));
    quit(&shell, cx);
}

#[gpui::test]
fn gh350_archive_named_marked_directory_navigates(cx: &mut TestAppContext) {
    marked_archive_named_directory(cx, false);
}

#[gpui::test]
fn gh350_offscreen_archive_named_marked_directory_navigates(cx: &mut TestAppContext) {
    marked_archive_named_directory(cx, true);
}
