//! Filtered directory pickers keep every footer control reachable at narrow and wide sizes.
use super::new_folder_tests::seed;
use super::tests::fixture::fixture;
use gpui::{px, size, Modifiers, TestAppContext};

fn filtered_footer(cx: &mut TestAppContext, width: f32, stacked: bool) {
    for mode in ["open", "savefiles"] {
        let directory = mode == "open";
        let request = format!(
            r#"{{"mode":"{mode}","directory":{directory},"folder":"/d","accept":"Use this directory as destination","filters":[{{"label":"Text","globs":["*.txt"]}}]}}"#
        );
        let mut f = fixture(cx, &request, None);
        seed(&mut f, "/d");
        f.cx.simulate_resize(size(px(width), px(440.)));
        f.cx.run_until_parked();
        let filter =
            f.cx.debug_bounds("picker-filter")
                .expect("the filter is drawn");
        let folder =
            f.cx.debug_bounds("picker-new-folder")
                .expect("New Folder is drawn");
        let cancel = f.cx.debug_bounds("picker-cancel").expect("Cancel is drawn");
        let answer =
            f.cx.debug_bounds("picker-answer")
                .expect("the caller's answer is drawn");
        for bounds in [filter, folder, cancel, answer] {
            assert!(
                bounds.left() >= px(0.)
                    && bounds.right() <= px(width)
                    && bounds.top() >= px(0.)
                    && bounds.bottom() <= px(440.),
                "{mode} at {width}: footer controls stay inside the window: {bounds:?}"
            );
        }
        assert!(
            folder.right() <= cancel.left() && cancel.right() <= answer.left(),
            "{mode} at {width}: New Folder, Cancel and the answer do not overlap"
        );
        if stacked {
            assert!(
                filter.bottom() <= folder.top(),
                "{mode} at {width}: the filter moves above the answer row instead of covering its buttons"
            );
        } else {
            assert!(
                folder.right() <= filter.left() && filter.right() <= cancel.left()
                    && filter.top() < folder.bottom() && folder.top() < filter.bottom(),
                "{mode} at {width}: a fitting filter shares the row without overlapping its neighbours"
            );
        }
        assert!(f.read(|picker| picker.browse.filename.is_none()));
        f.cx.simulate_click(filter.center(), Modifiers::none());
        f.cx.run_until_parked();
        assert!(
            f.cx.debug_bounds("picker-filter-menu").is_some(),
            "{mode} at {width}: the visible filter remains clickable"
        );
    }
}

#[gpui::test]
fn nf_filtered_directory_footer_stacks_without_covering_answers(cx: &mut TestAppContext) {
    for width in [640., 720.] {
        filtered_footer(cx, width, true);
    }
}

#[gpui::test]
fn nf_filtered_directory_footer_stays_inline_when_it_fits(cx: &mut TestAppContext) {
    filtered_footer(cx, 1100., false);
}
