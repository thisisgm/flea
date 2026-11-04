// Pinned menu tag reads keep metadata and xattrs inside the existing mount bound.
use super::{
    iomount,
    menu_actions::{self, Selected},
    opsreq::OpMsg,
    rowguard::stamped,
    tagsreq,
};
use std::sync::mpsc::Sender;

// Sample input: {"c":"menuaction","op":"tagstates","id":7,"tagId":2}
pub(super) fn ask(generation: u64, line: String, items: Vec<Selected>, tx: Sender<OpMsg>) {
    let id = crate::json::field_usize(&line, "tagId").unwrap_or(0);
    let body = iomount::mount_body();
    if let Some(first) = items.first() {
        if let Err(error) = iomount::call(&first.path, &body, "tagstates", || ()) {
            let line = menu_actions::response(&line, Err(error.msg));
            let _ = tx.send(OpMsg::Meta {
                line: stamped(line, generation),
            });
            return;
        }
    }
    std::thread::spawn(move || {
        let carried: Result<Vec<_>, String> = items
            .into_iter()
            .map(|item| {
                let path = item.path.clone();
                iomount::call(&path, &body, "tagstates", move || {
                    Ok(super::menutags_io::Pinned::open(&item)?.read_names())
                })
                .map_err(|error| error.msg)?
            })
            .collect();
        let reply = match carried {
            Ok(carried) => tagsreq::states_line_for(id, &carried),
            Err(error) => menu_actions::response(&line, Err(error)),
        };
        let _ = tx.send(OpMsg::Meta {
            line: stamped(reply, generation),
        });
    });
}
