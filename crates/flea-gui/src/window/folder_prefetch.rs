//! Bounded cache warming for a folder that stays under the pointer.
use crate::{
    backend::{Backend, Event},
    pane::requests::RowAction,
};
use flea::{
    jsondoc,
    protocol::{Reply, RowFacts},
};
use gpui::Context;
use std::time::{Duration, Instant};

const HOVER_DELAY: Duration = Duration::from_millis(350);
const ROW_LIMIT: usize = 48;
const DIRECTORY_SCAN_LIMIT: usize = 256;
const THUMB_LIMIT: usize = 32;
const SIZE_LIMIT: usize = 8;
const REPLY_LIMIT: Duration = Duration::from_secs(5);

impl crate::state::Shell {
    pub(crate) fn folder_prefetch_hover(
        &mut self,
        side: usize,
        path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let enabled = self
            .launch
            .settings
            .data
            .get("preview")
            .and_then(|value| value.get("prefetchOnHover"))
            .and_then(jsondoc::Json::as_bool)
            == Some(true);
        let path = enabled.then_some(path).flatten();
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        if host.folder_prefetch.as_ref().map(|(current, _)| current) == path.as_ref() {
            return;
        }
        host.folder_prefetch = None;
        let Some(path) = path else { return };
        let policy = crate::grid::element::pictures::Policy::read(&self.launch.settings.data);
        let executor = cx.background_executor().clone();
        let task_path = path.clone();
        let task = cx.spawn(async move |_, cx| {
            cx.background_executor().timer(HOVER_DELAY).await;
            let worker = executor.clone();
            let background = executor.spawn(async move {
                warm(&task_path, policy, worker).await;
            });
            background.await;
        });
        host.folder_prefetch = Some((path, task));
    }
}

async fn warm(
    path: &str,
    policy: crate::grid::element::pictures::Policy,
    executor: gpui::BackgroundExecutor,
) {
    if !small_folder(path) {
        return;
    }
    let Ok(backend) = Backend::start(path.to_owned(), None, 0) else {
        return;
    };
    let _ = backend.fsinfo();
    let mut total: Option<usize> = None;
    let mut listing = None;
    let mut rows = Vec::<(usize, RowFacts)>::new();
    let mut class = None;

    while rows.len() < total.unwrap_or(ROW_LIMIT).min(ROW_LIMIT) {
        match backend.replies.recv().await {
            Ok(Event::Reply(Reply::Listed { total: count, .. })) => total = Some(count),
            Ok(Event::Reply(Reply::Rows {
                start,
                rows: batch,
                listing: id,
            })) => {
                if batch.is_empty() {
                    break;
                }
                listing = Some(id);
                let limit = total.unwrap_or(ROW_LIMIT).min(ROW_LIMIT);
                rows.extend(
                    batch
                        .into_iter()
                        .take(limit.saturating_sub(rows.len()))
                        .enumerate()
                        .map(|(offset, row)| (start + offset, row)),
                );
                if rows.len() < limit {
                    let next = rows.last().map_or(0, |(index, _)| index + 1);
                    if backend
                        .window(next, (limit - rows.len()).min(ROW_LIMIT))
                        .is_err()
                    {
                        break;
                    }
                }
            }
            Ok(Event::Filesystem(facts)) => class = Some(facts.class),
            Ok(Event::FilesystemDetails { class: found, .. }) => class = Some(found),
            Ok(Event::Drained) | Err(_) => return,
            _ => {}
        }
    }

    if rows.is_empty() {
        return;
    }
    let class_deadline = Instant::now() + Duration::from_millis(400);
    while class.is_none() && Instant::now() < class_deadline {
        match backend.replies.try_recv() {
            Ok(Event::Filesystem(facts)) => class = Some(facts.class),
            Ok(Event::FilesystemDetails { class: found, .. }) => class = Some(found),
            Ok(Event::Drained) | Err(async_channel::TryRecvError::Closed) => break,
            _ => executor.timer(Duration::from_millis(20)).await,
        }
    }
    let sizes: Vec<usize> = rows
        .iter()
        .filter(|(_, row)| row.dir)
        .take(SIZE_LIMIT)
        .map(|(index, _)| *index)
        .collect();
    if let (Some(listing), false) = (listing, sizes.is_empty()) {
        let _ = backend.rows(
            "dirsize",
            RowAction {
                listing,
                rows: sizes.clone(),
            },
        );
    }
    let thumbnails: Vec<usize> = if let Some(class) = class.as_deref() {
        if policy.cache_only(class) {
            Vec::new()
        } else {
            rows.iter()
                .filter(|(_, row)| policy.allowed(row))
                .take(THUMB_LIMIT)
                .map(|(index, _)| *index)
                .collect()
        }
    } else {
        Vec::new()
    };
    if !thumbnails.is_empty() {
        let indices = thumbnails
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let line = format!(r#"{{"c":"thumb","rows":[{indices}],"cacheOnly":false}}"#);
        let _ = backend.operation(line);
    }

    let mut sizes_left = sizes.len();
    let mut thumbs_left = thumbnails.len();
    let deadline = Instant::now() + REPLY_LIMIT;
    while (sizes_left > 0 || thumbs_left > 0) && Instant::now() < deadline {
        match backend.replies.try_recv() {
            Ok(Event::ListDetails(crate::list::sizes::Reply::Sized { .. })) => {
                sizes_left = sizes_left.saturating_sub(1)
            }
            Ok(Event::Producer(line)) if is_thumb_reply(&line) => {
                thumbs_left = thumbs_left.saturating_sub(1)
            }
            Ok(Event::Drained) | Err(async_channel::TryRecvError::Closed) => break,
            _ => {}
        }
        if sizes_left > 0 || thumbs_left > 0 {
            executor.timer(Duration::from_millis(25)).await;
        }
    }
    let _ = backend.quiesce();
}

fn small_folder(path: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    entries.take(DIRECTORY_SCAN_LIMIT + 1).count() <= DIRECTORY_SCAN_LIMIT
}

fn is_thumb_reply(line: &str) -> bool {
    jsondoc::parse(line)
        .ok()
        .and_then(|doc| {
            doc.get("t")
                .and_then(jsondoc::Json::as_str)
                .map(str::to_owned)
        })
        .is_some_and(|kind| kind == "thumbed")
}
