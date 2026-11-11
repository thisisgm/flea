//! The production window opens the output card from Ctrl+P and the menu, and serves its effects with no test subscriber.
use crate::{
    backend::{Backend, Event},
    g2::{
        context_menus::join::{Captured, Chosen},
        print_convert::model::Card,
    },
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use flea::protocol::RowFacts;
use gpui::{AppContext, TestAppContext, VisualTestContext, Window, WindowHandle};
use std::{cell::Cell, rc::Rc};

#[path = "output_tests/combine_preview.rs"]
mod combine_preview;
#[path = "output_tests/result_selection_tests.rs"]
mod result_selection;

fn open(cx: &mut TestAppContext, backend: Backend) -> WindowHandle<Root> {
    cx.add_window(|_, cx| {
        let shell = cx.new(|cx| shell_with(backend, cx));
        Root { shell }
    })
}

fn shell<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    update: impl FnOnce(&mut Shell, &mut Window, &mut gpui::Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell
                .clone()
                .update(cx, |shell, cx| update(shell, window, cx))
        })
        .expect("the window is open")
}

fn frame(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .expect("the window is open");
}

fn focus_shell(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    let focus = shell(cx, handle, |shell, _, _| shell.focus.clone());
    cx.update_window(handle.into(), |_, window, cx| window.focus(&focus, cx))
        .expect("the window is open");
}

fn shell_focused(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> bool {
    shell(cx, handle, |shell, window, _| {
        shell.focus.is_focused(window)
    })
}

// The pane's held rows become exactly these, on its current listing.
fn hold(shell: &mut Shell, rows: Vec<RowFacts>) {
    let pane = &mut shell.panes[0].as_mut().unwrap().pane;
    pane.held = Rc::new(crate::list::pane::Held {
        listing: pane.held.listing,
        total: rows.len(),
        start: 0,
        rows,
    });
}

fn row(name: &str, icon: &str) -> RowFacts {
    RowFacts {
        name: name.into(),
        icon: icon.into(),
        mode: 0o100644,
        ..Default::default()
    }
}

// The next request the pane's backend received whose command is `command`.
fn sent(requests: &mut impl FnMut() -> Option<String>, command: &str) -> Option<String> {
    let wanted = format!("\"c\":\"{command}\"");
    std::iter::from_fn(&mut *requests).find(|line| line.contains(&wanted))
}

fn field(line: &str, key: &str) -> u64 {
    flea::json::field_usize(line, key).expect("a numeric field") as u64
}

#[gpui::test]
fn m9_integ_output_effects_use_real_pages_thumbnails_and_origin(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let handle = open(cx, backend);
    shell(cx, handle, |shell, _, _| {
        let rows = vec![
            row("report.pdf", "x-office-document"),
            row("a.png", "image-x-generic"),
        ];
        hold(shell, rows);
        shell.panes[0].as_mut().unwrap().pane.cursor = 0;
    });
    // The root's first frame mounts the production menu and card observers.
    frame(cx, handle);
    focus_shell(cx, handle);
    while requests().is_some() {}

    // Ctrl+P on the listing is the Print card's real producer; nothing in this test subscribes to ActionEvent.
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("ctrl-p");
    let query = sent(&mut requests, "printoptions").expect("Ctrl+P queries the printers");
    let view = shell(cx, handle, |shell, _, _| shell.output.view.clone()).expect("a card view");
    assert!(shell(cx, handle, |shell, _, cx| shell.output_showing(cx)));
    assert!(
        !shell_focused(cx, handle),
        "the open card takes the keyboard"
    );
    let reply = |id: u64| {
        format!(
            r#"{{"t":"printoptions","id":{id},"ok":true,"destinations":[{{"name":"Office","default":true}}],"printer":"Office","duplex":true,"twoSided":false,"color":true,"grayscale":false,"message":"","setupArgv":[],"err":""}}"#
        )
    };
    let id = field(&query, "id");
    shell(cx, handle, |shell, window, cx| {
        // A stale ticket and a reply on a pane that does not own the card both stay unconsumed.
        assert!(
            !shell.output_line(0, &reply(id + 7), cx),
            "a stale reply is refused"
        );
        assert!(
            !shell.output_line(1, &reply(id), cx),
            "only the owner pane answers"
        );
        shell.receive(0, Event::Producer(reply(id)), window, cx);
    });
    let queried = view.read_with(cx, |view, _| match &view.model.card {
        Some(Card::Print(p)) => p.queried && p.printer == "Office",
        _ => false,
    });
    assert!(queried, "the owner's printoptions reply lands in the card");

    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("enter");
    let print = sent(&mut requests, "print").expect("Print sends the print request");
    assert!(
        print.contains(r#""paths":["/files/report.pdf"]"#),
        "{print}"
    );
    let request = field(&print, "requestId");
    shell(cx, handle, |shell, window, cx| {
        for line in [
            format!(
                r#"{{"t":"printstarted","id":9,"requestId":{request},"title":"Print report.pdf"}}"#
            ),
            format!(
                r#"{{"t":"printed","id":9,"requestId":{request},"path":"/files/report.pdf","ok":true,"message":""}}"#
            ),
            format!(
                r#"{{"t":"printdone","id":9,"requestId":{request},"ok":true,"sent":1,"message":"","err":""}}"#
            ),
        ] {
            shell.receive(0, Event::Producer(line), window, cx);
        }
    });
    assert!(!shell(cx, handle, |shell, _, cx| shell.output_showing(cx)));
    assert!(
        shell_focused(cx, handle),
        "the started print hands the keyboard back to its origin"
    );
    let notice = shell(cx, handle, |shell, _, cx| {
        shell.status_messages(cx, |messages, _| messages.notice().to_owned())
    });
    assert!(
        notice.contains("report.pdf") && notice.contains("Office"),
        "{notice}"
    );

    // The menu's validated snapshot opens Convert; its PDF page count comes from the preview's PDF reader.
    shell(cx, handle, |shell, _, cx| {
        let listing = shell.panes[0].as_ref().unwrap().pane.held.listing;
        cx.emit(Chosen {
            captured: Captured {
                side: 0,
                listing,
                rows: vec![0],
                cursor: 0,
                path: "/files".into(),
                identity: String::new(),
                id: 41,
                ready: true,
                kinds: Vec::new(),
                background: false,
                target: None,
            },
            action: "convert".into(),
            leaf: String::new(),
            paths: vec!["/files/report.pdf".into()],
            claimed: Rc::new(Cell::new(false)),
        });
    });
    cx.run_until_parked();
    let (pending, error, thumbnails) = view.read_with(cx, |view, _| match &view.model.card {
        Some(Card::Convert(c)) => (
            c.count_pending.is_some(),
            view.model.error.clone(),
            view.thumbnails.len(),
        ),
        _ => panic!("the menu choice opens the Convert card"),
    });
    // /files/report.pdf does not exist, so the reader's real answer is an unreadable document.
    assert!(!pending, "the page count request was answered");
    assert!(error.contains("page count"), "{error}");
    assert_eq!(
        thumbnails, 0,
        "no held picture set is forged without its producer"
    );
    let selection_menu =
        view.read_with(cx, |view, _| view.model.selection.as_ref().map(|s| s.menu));
    assert_eq!(
        selection_menu,
        Some(41),
        "the request keeps the menu snapshot"
    );

    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("escape");
    assert!(!shell(cx, handle, |shell, _, cx| shell.output_showing(cx)));
    assert!(
        shell_focused(cx, handle),
        "closing restores the card's origin focus"
    );
}

#[gpui::test]
fn menu_context_reads_output_kinds_from_held_rows(cx: &mut TestAppContext) {
    use flea::ui_logic_print_convert::output::Kind;
    assert_eq!(
        super::output::kind(&row("a.png", "image-x-generic")),
        Kind::Image
    );
    assert_eq!(
        super::output::kind(&row("r.PDF", "x-office-document")),
        Kind::Pdf
    );
    assert_eq!(
        super::output::kind(&row("s.odt", "x-office-document")),
        Kind::Office
    );
    assert_eq!(
        super::output::kind(&row("n.txt", "text-x-generic")),
        Kind::Text
    );
    assert_eq!(
        super::output::kind(&row("x.bin", "application-x-generic")),
        Kind::Other
    );
    let (backend, _requests) = Backend::test_lines();
    let handle = open(cx, backend);
    shell(cx, handle, |shell, window, cx| {
        let rows = vec![
            row("a.png", "image-x-generic"),
            row("b.png", "image-x-generic"),
        ];
        hold(shell, rows);
        shell.panes[0].as_mut().unwrap().selected = vec![0, 1];
        shell.open_menu(0, Some(0), (10., 10.), window, cx);
        let context = &shell.menu.context;
        assert!(context.row_is_image && context.printable && context.can_convert);
        assert_eq!(context.selection_images, 2);
        // A backend that reports no converter hides Convert and Combine on the next open.
        shell.output_line(0, r#"{"t":"formats","archive":[],"convert":false}"#, cx);
        shell.open_menu(0, Some(0), (10., 10.), window, cx);
        assert!(!shell.menu.context.can_convert);
    });
}

#[gpui::test]
fn a_pdf_or_office_row_offers_convert(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_lines();
    let handle = open(cx, backend);
    shell(cx, handle, |shell, window, cx| {
        hold(
            shell,
            vec![
                row("garden-report.pdf", "x-office-document"),
                row("garden-report.docx", "x-office-document"),
            ],
        );
        for (index, name) in [(0, "garden-report.pdf"), (1, "garden-report.docx")] {
            shell.panes[0].as_mut().unwrap().selected = vec![index];
            shell.open_menu(0, Some(index), (10., 10.), window, cx);
            assert_eq!(shell.menu.context.row_name, name);
            assert!(shell.menu.context.row_is_document);
            let entries = flea::ui_logic_context_menus::build::listing_entries(&shell.menu.context);
            assert!(
                entries.iter().any(|e| e.action == "convert"),
                "row {index} offers Convert"
            );
        }
    });
}
