//! The Trash view hands journal actions to the window, including when no rows remain.
use super::{
    dates_tests::english,
    view::{Keys, Press, Signal, View},
    words::{route, Route},
};

#[test]
fn trash_undo_routes_to_the_window() {
    for action in ["undo", "redo"] {
        assert_eq!(
            route(action),
            Route::Direct,
            "{action} belongs to the window journal"
        );
        let mut view = View::new(31.);
        view.press(Press::Action(action), false, 0, &english(), Keys::default());
        assert_eq!(view.signals(), [Signal::Action(action.into())]);
        assert!(
            view.drain().is_empty(),
            "refresh waits for journal completion"
        );
    }
    for action in ["trashArm", "deletePermanently", "restoreTrashSelection"] {
        assert_eq!(
            route(action),
            Route::Trash,
            "{action} retains its Trash meaning"
        );
    }
}
