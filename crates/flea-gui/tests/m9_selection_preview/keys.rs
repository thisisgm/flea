//! Default keys reach the mounted preview through the Shell's own key dispatch.
use super::*;
use flea::jsondoc::Json;

fn setting(host: &mut Host, path: &[&str]) -> Option<String> {
    host.window
        .update(&mut host.cx, |shell, _, _| {
            let mut value = Some(&shell.launch.settings.data);
            for key in path {
                value = value.and_then(|v| v.get(key));
            }
            value.map(|v| match v {
                Json::Bool(on) => on.to_string(),
                Json::Str(text) => text.clone(),
                _ => String::new(),
            })
        })
        .unwrap()
}

fn scroll(host: &mut Host) -> String {
    let state = host.call_args("pdfState", &["false"]);
    let tail = state.split("\"scrollY\":").nth(1).unwrap_or_default();
    tail.split([',', '}'])
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned()
}

pub fn keys() {
    let _lock = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut host = Host::new(View::Columns);
    host.until("columnPdfLoaded", "true");
    assert_eq!(scroll(&mut host), "0");
    host.cx
        .simulate_keystrokes(host.window.into(), "shift-f->F");
    host.draw();
    assert_ne!(
        scroll(&mut host),
        "0",
        "F turns the column's continuous PDF forward"
    );
    host.cx
        .simulate_keystrokes(host.window.into(), "shift-b->B");
    host.draw();
    assert_eq!(scroll(&mut host), "0", "B turns it back to the first page");
    let compact = setting(&mut host, &["previewFacts"]).as_deref() != Some("full");
    host.cx.simulate_keystrokes(host.window.into(), "i->i");
    host.draw();
    let expected = if compact { "full" } else { "compact" };
    assert_eq!(
        setting(&mut host, &["previewFacts"]).as_deref(),
        Some(expected)
    );
    host.cx.simulate_keystrokes(host.window.into(), "alt-p");
    host.draw();
    assert_eq!(
        setting(&mut host, &["preview", "column"]).as_deref(),
        Some("false")
    );
    host.until("previewColumnState", "");
    host.cx.simulate_keystrokes(host.window.into(), "alt-p");
    host.draw();
    assert_eq!(
        setting(&mut host, &["preview", "column"]).as_deref(),
        Some("true")
    );
    host.until("columnPdfLoaded", "true");
}

/// List and Grid keep the whole listing, as v0.3.12 does; Columns mounts the same frame again.
pub(super) fn list_and_grid(host: &mut Host) {
    for mode in [View::List, View::Grid, View::Columns] {
        host.window
            .update(&mut host.cx, |shell, window, cx| {
                shell.set_view(mode, window, cx)
            })
            .unwrap();
        let state = if mode == View::Columns { "pdf" } else { "" };
        host.until("previewColumnState", state);
    }
    host.until("columnPdfLoaded", "true");
}
