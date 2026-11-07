//! c over a hidden Copy As row opens its flyout alone; in a history listing such as Recent that flyout hides Relative Path as the drawn row's does.
use super::menu::{Face, Outcome};
use super::tests::{file_rows, window, with, WIDE};
use gpui::TestAppContext;

fn lone_leaves(cx: &mut TestAppContext, history: bool) -> Vec<String> {
    let (handle, _) = window(cx);
    with(cx, handle, |menu, env, window| {
        let rows: Vec<_> = file_rows()
            .into_iter()
            .filter(|row| row.action != "copyAs")
            .collect();
        menu.open(Face::File, rows, (40., 40.), WIDE, "a", env, window);
        menu.history = history;
        assert_eq!(
            menu.open_submenu_for("copyAs", false, env, window),
            Outcome::Redraw
        );
        let flyout = menu.flyout.as_ref().unwrap();
        assert!(flyout.parent.is_none(), "the flyout opens alone");
        flyout
            .card
            .entries
            .iter()
            .map(|leaf| leaf.id.to_string())
            .collect()
    })
}

#[gpui::test]
fn a_lone_copy_as_flyout_hides_relative_path_only_in_a_history(cx: &mut TestAppContext) {
    assert_eq!(
        lone_leaves(cx, false).join(","),
        "copyPath,copyRelative,copyName,copyStem,copydirpath,copyUri,copyQuoted"
    );
    assert_eq!(
        lone_leaves(cx, true).join(","),
        "copyPath,copyName,copyStem,copydirpath,copyUri,copyQuoted"
    );
}
