//! Native registration and payload retain the current router and cancellation owners.
use super::*;

impl Shell {
    /// Registers this window with the process router and follows the drops it accepts; once per window.
    pub fn follow_file_drags(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = window_key(window);
        if cx.default_global::<Drags>().clients.contains_key(&key) {
            return;
        }
        let (send, receive) = async_channel::unbounded();
        let held = cx
            .global_mut::<Drags>()
            .held
            .entry(key)
            .or_default()
            .clone();
        let shared = router::shared(cx);
        let hover = cx
            .global_mut::<Drags>()
            .hovers
            .entry(key)
            .or_default()
            .clone();
        let (ended, receive_end) = async_channel::unbounded();
        cx.spawn_in(window, async move |this, cx| {
            while receive_end.recv().await.is_ok() {
                let _ = this.update_in(cx, |shell, window, cx| shell.settle_row_drag(window, cx));
            }
        })
        .detach();
        let registration = shared.register(Rc::new(Files {
            window: key,
            send,
            held,
            hover,
            ended: Some(ended),
        }));
        cx.global_mut::<Drags>().clients.insert(key, registration);
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(arrived) = receive.recv().await {
                let _ = this.update_in(cx, |shell, window, cx| {
                    shell.land_arrived(arrived, window, cx)
                });
            }
        })
        .detach();
    }

    /// The live lift and its captured flock are handed to the platform together.
    pub fn file_drag_payload(&self, window: &Window, cx: &App) -> Option<ExternalDragPayload> {
        let drags = cx.try_global::<Drags>()?;
        let key = window_key(window);
        let payload = drags.lifts.get(&key)?.payload();
        Some(
            drags
                .flocks
                .get(&key)
                .map_or(payload.clone(), |flock| flock.read(cx).native(payload)),
        )
    }

    /// Escape during this window's own drag: the drag ends where it started and nothing moves.
    pub fn escape_file_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let key = window_key(window);
        let lifted = cx.default_global::<Drags>().take(key).is_some();
        self.clear_file_rest(window, cx);
        self.clear_file_cue(window, cx);
        self.clear_rail_aim(window, cx);
        lifted && window.cancel_drag(cx)
    }
}
