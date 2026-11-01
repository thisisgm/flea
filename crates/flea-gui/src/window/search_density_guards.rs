//! Private guards for the joined density test fixture.
use super::*;

pub(super) fn open_density(cx: &mut TestAppContext) -> (Handle, impl FnMut() -> Option<String>) {
    let (handle, read) = open(cx);
    edit(cx, handle, |shell, _, _| {
        // The shared fixture stores {}; production settings loading supplies the full defaults.
        shell.launch.settings =
            crate::window::startup::Settings::from_json(flea::uistate::from_file("{}"));
        assert!(!shell.panes[0].as_ref().unwrap().pane.busy());
        assert_eq!(shell.settings.relist, [false, false]);
    });
    (handle, read)
}

pub(super) fn start_search(cx: &mut TestAppContext, handle: Handle, side: usize, origin: View) {
    edit(cx, handle, |shell, window, cx| {
        assert_eq!(shell.view, origin);
        shell.focus_side = side;
        let host = shell.panes[side].as_ref().unwrap();
        assert!(
            !host.pane.busy(),
            "density fixture must have no unanswered ordinary relist"
        );
        assert!(!host.search.owner.active());
        assert_eq!(
            host.search.owner.session.mode,
            flea::ui_logic_sr1::Mode::Off
        );
        assert!(shell.search_action("search", window, cx));
        let host = shell.panes[side].as_ref().unwrap();
        assert!(
            host.search.owner.active(),
            "a consumed action alone does not prove search opened"
        );
        assert!(host.search.owner.typing());
        assert_eq!(
            host.search.owner.session.mode,
            flea::ui_logic_sr1::Mode::Typing
        );
        assert_eq!(host.search.owner.session.scope, Scope::Home);
        assert!(host.search.owner.session.query.is_empty());
        assert_eq!(
            host.search.owner.session.gen,
            host.search.owner.session.opened_gen
        );
        assert!(host.pane.swap.session.open);
        assert_eq!(
            shell.view, origin,
            "a names search keeps the view it opened in"
        );
        assert!(shell.focus.is_focused(window));
    });
}

pub(super) fn type_query(
    cx: &mut TestAppContext,
    handle: Handle,
    side: usize,
    scope: Scope,
) -> u64 {
    if scope == Scope::Folder {
        key(cx, handle, "tab", None);
    }
    let before = edit(cx, handle, |shell, window, _| {
        let host = shell.panes[side].as_ref().unwrap();
        assert_eq!(shell.focus_side, side);
        assert!(shell.focus.is_focused(window));
        assert!(host.search.owner.active());
        assert!(host.search.owner.typing());
        assert_eq!(
            host.search.owner.session.scope, scope,
            "Tab must reach the active query owner"
        );
        assert!(host.search.owner.session.query.is_empty());
        host.search.owner.session.gen
    });
    key(cx, handle, "x", Some("x"));
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[side].as_ref().unwrap();
        let session = &host.search.owner.session;
        assert!(host.search.owner.active());
        assert_eq!(session.mode, flea::ui_logic_sr1::Mode::Typing);
        assert_eq!(session.scope, scope);
        assert_eq!(
            session.query, "x",
            "the real Shell key route must type the query"
        );
        assert_eq!(session.gen, before + 1);
        assert!(session.gen > session.opened_gen);
        assert_eq!(session.stamps.gen, session.gen);
        assert!(host.pane.swap.session.open);
        assert_eq!(host.pane.swap.session.generation, session.gen as usize);
        assert_eq!(host.pane.swap.session.query, "x");
        session.gen
    })
}

pub(super) fn query_identity(
    cx: &mut TestAppContext,
    handle: Handle,
    side: usize,
) -> (u64, flea::ui_logic_sr1::Mode, Scope) {
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[side].as_ref().unwrap();
        assert!(host.search.owner.active());
        assert_eq!(host.search.owner.session.query, "x");
        let session = &host.search.owner.session;
        (session.gen, session.mode, session.scope)
    })
}
