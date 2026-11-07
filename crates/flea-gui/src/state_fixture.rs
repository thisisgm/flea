use super::*;

pub(crate) fn shell_with(backend: Backend, cx: &mut Context<Shell>) -> Shell {
    let locale = Snapshot {
        revision: 1,
        catalog: Arc::new(crate::test_catalogs::english()),
    };
    let settings = Settings::from_json(flea::jsondoc::parse("{}").unwrap());
    let launch = Launch::resolve(
        settings,
        Arguments {
            path: Some("/files".into()),
            select: None,
        },
        "/home/test",
    );
    let mut host = PaneHost::new("/files".into(), None, 620., &locale);
    host.backend = Some(backend);
    host.pane
        .receive(Reply::Listed {
            path: "/files".into(),
            total: 1000,
        })
        .unwrap();
    host.pane
        .receive(Reply::Rows {
            start: 0,
            rows: vec![flea::protocol::RowFacts::default(); 320],
            listing: 2,
        })
        .unwrap();
    let mut lifecycle = Lifecycle::default();
    lifecycle.attach(0);
    Shell {
        capture: crate::capture::Capture::default(),
        visual: crate::visual::Visual::default(),
        path_labels: launch.paths.clone().map(SharedString::from),
        launch,
        locale: locale.clone(),
        labels: labels(&locale),
        slots: Slots::default(),
        chrome: Default::default(),
        chrome_move: Default::default(),
        lifecycle,
        view: View::List,
        focus_side: 0,
        grows: Default::default(),
        bands: Default::default(),
        keyboard: Default::default(),
        input: Default::default(),
        panes: [Some(host), None],
        surfaces: Default::default(),
        menu: Default::default(),
        overlay: None,
        quick_look: None,
        output: Default::default(),
        operations: Default::default(),
        rename: Default::default(),
        clipboard: Default::default(),
        settings: Default::default(),
        trash: None,
        rail: None,
        corner: crate::g2::st1::seams::COMPOSITOR_CORNER_DEFAULT,
        focus: cx.focus_handle(),
        seam: Seam::new(),
        ipc_events: None,
        _ipc_reader: None,
        ipc_cold: false,
        _locale_reader: None,
        locale_service: None,
        presentation_service: None,
        _presentation_reader: None,
        font_key: None,
        _app_quit: None,
        deadlines: [None, None],
        midnight: None,
        opener: Default::default(),
        dual_focus: crate::dual::focus::Focus::new(cx),
        dual_pending: false,
        dual_written: None,
        dual_drag: None,
        dual_hover: false,
        sidebar: Default::default(),
        dual_blur: None,
    }
}
