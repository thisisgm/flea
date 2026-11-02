//! Catalogue discovery has its own bounded slot, so a metadata query cannot hold a chosen menu operation.
use super::*;

impl MenuActions {
    pub fn new(replies: Sender<OpMsg>) -> Self {
        // One pending request bounds repeated activation while an operation runs.
        let slot = Arc::new(MenuSlot::default());
        let output = replies.clone();
        let queued = Arc::clone(&slot);
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let published = Arc::clone(&snapshot);
        let registry = Registry::default();
        let queries = registry.clone();
        let restoration = Arc::new(Mutex::new((0, Vec::new())));
        let completed = Arc::clone(&restoration);
        std::thread::spawn(move || {
            let _close = CloseOnExit(Arc::clone(&queued));
            while let Some((line, paths, cursor, cancel)) = queued.take() {
                let mut state = published.lock().unwrap().clone();
                if field_str(&line, "op").as_deref() == Some("properties") && state.items.len() != 1
                {
                    if super::super::menuproperties::serve(
                        &line,
                        state.id,
                        &state.items,
                        &output,
                        &cancel,
                    ) {
                        continue;
                    } else {
                        break;
                    }
                }
                if field_str(&line, "op").as_deref() == Some("contents") {
                    if super::super::menucontents::serve(
                        &line,
                        state.id,
                        &state.items,
                        &output,
                        &cancel,
                    ) {
                        continue;
                    } else {
                        break;
                    }
                }
                if field_str(&line, "op").as_deref() == Some("facts") {
                    if super::super::menufacts::serve(
                        &line,
                        state.id,
                        &state.items,
                        &output,
                        &cancel,
                    ) {
                        continue;
                    } else {
                        break;
                    }
                }
                let mut reply = if cancel.check().is_ok() {
                    state.handle_request(&line, paths, cursor.as_deref(), &queries, &cancel)
                } else {
                    response(&line, Err("Menu request cancelled.".into()))
                };
                if cancel.check().is_err() && field_str(&line, "op").as_deref() != Some("delete") {
                    reply.insert_str(reply.len() - 1, r#", "cancelled":true"#);
                }
                if field_str(&line, "op").as_deref() == Some("delete") {
                    // Close expires mutation immediately; the completed operation still identifies survivors for reselection.
                    *completed.lock().unwrap() = (state.id, state.items.clone());
                }
                if matches!(
                    field_str(&line, "op").as_deref(),
                    Some(
                        "snapshot"
                            | "close"
                            | "prepareDelete"
                            | "refreshDelete"
                            | "delete"
                            | "providerDestination"
                            | "activate"
                    )
                ) {
                    publish_snapshot(published.lock().unwrap(), state, &cancel);
                }
                let message = if field_str(&line, "op").as_deref() == Some("delete") {
                    OpMsg::MenuDeleteDone { line: reply }
                } else {
                    OpMsg::Meta { line: reply }
                };
                if output.send(message).is_err() {
                    break;
                }
            }
        });
        let catalogue_registry = Registry::default();
        let catalogue = catalogue(
            Arc::clone(&snapshot),
            replies.clone(),
            catalogue_registry.clone(),
        );
        Self {
            slot,
            catalogue,
            replies,
            snapshot,
            cancellation: Mutex::new(Cancellation::default()),
            registry,
            catalogue_registry,
            requested_id: AtomicUsize::new(0),
            restoration,
        }
    }
}

// Sample catalogue request: {"c":"menuaction","op":"applications","id":7}.
fn catalogue(
    snapshot: Arc<Mutex<Snapshot>>,
    replies: Sender<OpMsg>,
    registry: Registry,
) -> Arc<MenuSlot> {
    let slot = Arc::new(MenuSlot::default());
    let queued = Arc::clone(&slot);
    std::thread::spawn(move || {
        let _close = CloseOnExit(Arc::clone(&queued));
        while let Some((line, paths, cursor, cancel)) = queued.take() {
            let mut state = snapshot.lock().unwrap().clone();
            let mut reply = if cancel.check().is_ok() {
                state.handle_request(&line, paths, cursor.as_deref(), &registry, &cancel)
            } else {
                response(&line, Err("Menu request cancelled.".into()))
            };
            if cancel.check().is_err() {
                reply.insert_str(reply.len() - 1, r#", "cancelled":true"#);
            }
            if replies.send(OpMsg::Meta { line: reply }).is_err() {
                break;
            }
        }
    });
    slot
}

#[cfg(test)]
#[path = "menu_validation_tests.rs"]
mod tests;
