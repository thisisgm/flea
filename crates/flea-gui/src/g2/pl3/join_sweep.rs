//! The opt-in expiry sweep starts after the first window has had two seconds to present.
use crate::state::Shell;
use flea::jsondoc::Json;
use gpui::{Context, Window};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(super) fn wall_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_millis().min(i64::MAX as u128) as i64)
}

impl Shell {
    pub(super) fn schedule_trash_sweep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let timer = cx.spawn_in(window, async move |shell, cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            let _ = shell.update_in(cx, |shell, window, cx| {
                if shell.lifecycle.phase() != crate::window::lifecycle::Phase::Open {
                    return;
                }
                let data = &shell.launch.settings.data;
                let enabled = data.get("trashAutoEmpty").and_then(Json::as_bool) == Some(true);
                let day = data
                    .get("trashSweptOn")
                    .and_then(Json::as_f64)
                    .unwrap_or(0.) as i64;
                if let Some(host) = &mut shell.trash {
                    if !host.pane.read(cx).view().opened {
                        host.backend_side = 0;
                    }
                    host.pane.update(cx, |pane, cx| {
                        pane.host.sweep(wall_ms(), enabled, day);
                        pane.pump(cx);
                    });
                }
                shell.flush_trash(window, cx);
            });
        });
        self.trash.as_mut().expect("mounted Trash").sweep_timer = Some(timer);
    }

    pub(super) fn record_trash_sweep(
        &mut self,
        day: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.change_settings(
            Json::Obj(vec![("trashSweptOn".into(), Json::Num(day.to_string()))]),
            window,
            cx,
        );
    }
}
