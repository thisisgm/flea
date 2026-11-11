//! One interactive view around the existing custom tab painter.
use super::{
    bar::{Frame, Hit, Labels, StripElement},
    transfer,
};
use crate::{state::Shell, window::startup::View};
use gpui::{
    div, prelude::*, AppContext, Bounds, Context, MouseButton, Pixels, Point, Render, WeakEntity,
    Window,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub(crate) struct Strip {
    pub frame: Frame,
    pub labels: Rc<RefCell<Labels>>,
    pub bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub drop_zone: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub shell: WeakEntity<Shell>,
    press: Option<super::atomic::Press>,
    frames: Rc<RefCell<super::held::StripFrames>>,
    pub(super) tip: super::icons::Tip,
}

#[derive(Clone)]
struct Gesture;
struct Ghost;
impl Render for Ghost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl Render for Strip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Only a running screen reader pays for the accessible controls.
        let controls = window
            .is_a11y_active()
            .then(|| self.shell.upgrade())
            .flatten()
            .and_then(|shell| {
                super::accessibility::Controls::new(self.shell.clone(), shell.read(cx))
            });
        let geometry = self.bounds.clone();
        let drop_zone = self.drop_zone.clone();
        let drag = self.shell.clone();
        let dragged_view = cx.entity().downgrade();
        let payload = self.shell.clone();
        div()
            .on_children_prepainted(move |bounds, _, _| {
                let bounds = bounds.first().copied();
                geometry.set(bounds);
                drop_zone.set(bounds);
            })
            .id("native-tab-strip")
            .size_full()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|strip, event: &gpui::MouseDownEvent, _, cx| {
                    let _ = strip.shell.update(cx, |shell, _| {
                        strip.press = shell.tab_press(event.position);
                    });
                }),
            )
            .on_click(cx.listener(|strip, event: &gpui::ClickEvent, window, cx| {
                if !event.is_right_click() && !event.is_middle_click() {
                    let press = strip.press.take();
                    let _ = strip.shell.update(cx, |shell, cx| {
                        if let Some(press) = press {
                            let hit = shell.tab_pressed_hit(press);
                            shell.tab_tap_hit(hit, false, window, cx);
                        }
                    });
                }
            }))
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|strip, event: &gpui::MouseDownEvent, window, cx| {
                    let _ = strip.shell.update(cx, |shell, cx| {
                        shell.tab_tap(event.position, true, window, cx)
                    });
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|strip, event: &gpui::MouseDownEvent, window, cx| {
                    let _ = strip.shell.update(cx, |shell, cx| {
                        shell.open_tab_menu(event.position, window, cx)
                    });
                }),
            )
            .on_mouse_move(cx.listener(|strip, event: &gpui::MouseMoveEvent, _, cx| {
                let _ = strip.shell.update(cx, |shell, cx| {
                    if shell.tab_holding() {
                        return;
                    }
                    let x = shell.tab_x(event.position);
                    if shell
                        .strip
                        .bar
                        .hovered(Some(x), shell.strip.tabs[shell.focus_side].count())
                    {
                        cx.notify();
                    }
                });
            }))
            .on_hover(cx.listener(|strip, hovered: &bool, _, cx| {
                if !hovered {
                    strip.tip_left(cx);
                    let _ = strip.shell.update(cx, |shell, cx| {
                        if shell
                            .strip
                            .bar
                            .hovered(None, shell.strip.tabs[shell.focus_side].count())
                        {
                            cx.notify();
                        }
                    });
                }
            }))
            .on_drag(Gesture, move |_, _, window, cx| {
                let press = dragged_view
                    .update(cx, |strip, _| strip.press.take())
                    .ok()
                    .flatten();
                if let Some(press) = press {
                    let _ = drag.update(cx, |shell, cx| {
                        if let Hit::Tab(index) = shell.tab_pressed_hit(press) {
                            let now = shell.tab_now();
                            shell.with_tabs(|bar, panes| bar.tab_lift_began(panes, index, now));
                            shell.begin_tab_geometry(cx);
                            shell.tabs_changed(window, cx);
                        }
                    });
                }
                cx.new(|_| Ghost)
            })
            .external_drag_payload::<Gesture>(move |_, _, cx| {
                payload
                    .update(cx, |shell, cx| shell.tab_native_payload(cx))
                    .ok()
                    .flatten()
            })
            .on_drag_move::<Gesture>(cx.listener(
                |strip, event: &gpui::DragMoveEvent<Gesture>, _, cx| {
                    let _ = strip.shell.update(cx, |shell, cx| {
                        let x = shell.tab_x(event.event.position);
                        let count = shell.strip.tabs[shell.focus_side].count();
                        shell
                            .strip
                            .bar
                            .drag_moved(f64::from(x - super::bar::PAD_X), count);
                        cx.notify();
                    });
                },
            ))
            .on_drop::<Gesture>(cx.listener(|strip, _, window, cx| {
                let _ = strip
                    .shell
                    .update(cx, |shell, cx| shell.end_tab_drag(window, cx));
            }))
            .on_drop::<crate::g2::drag_drop::join::Seed>(cx.listener(|strip, _, window, cx| {
                let _ = strip.shell.update(cx, |shell, cx| {
                    shell.land_own_drag(shell.focus_side, window, cx);
                });
            }))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|strip, _, window, cx| {
                    let _ = strip.shell.update(cx, |shell, cx| {
                        if shell.strip.bar.drag_from.is_some() && !shell.strip.bar.out_active {
                            shell.end_tab_drag(window, cx);
                        }
                    });
                }),
            )
            .child(
                StripElement::new(self.frame.clone(), self.labels.clone())
                    .with_feedback(self.frames.clone())
                    .with_controls(controls),
            )
            .map(|root| self.with_tip(root, cx))
    }
}

impl Shell {
    pub(crate) fn tab_now(&self) -> u64 {
        self.panes[self.focus_side]
            .as_ref()
            .map_or(0, |host| host.pane.now())
    }
    pub(crate) fn tab_x(&self, point: Point<Pixels>) -> f32 {
        f32::from(point.x)
            - self
                .strip
                .bounds
                .get()
                .map_or(0., |bounds| f32::from(bounds.origin.x))
    }

    pub(crate) fn sync_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.strip.view.is_none() && self.slots.tabs.is_some() {
            return;
        }
        if self.strip.frames.borrow().observer.is_none() {
            let frames = self.strip.frames.clone();
            let observer = window.observe_frame_feedback(cx, move |event, _, _| {
                frames.borrow_mut().feedback(event);
            });
            self.strip.frames.borrow_mut().observer = observer;
        }
        let font_changed = self.sync_tab_typeface();
        let side = self.focus_side;
        let count = self.strip.tabs[side].count();
        self.strip
            .capacity
            .set(count < super::MAX || self.strip.bar.out_active);
        let holding = self.tab_holding();
        let visible = self.has_tabs();
        self.strip.bar.stop_focused = visible && self.dual_focus.strip.is_focused(window);
        self.strip.bar.focused_index = self.strip.bar.focused_index.min(count);
        {
            let mut frames = self.strip.frames.borrow_mut();
            frames.follow(holding);
            frames.visible = visible;
        }
        if holding {
            self.reflow_held_tabs(window, font_changed, cx);
            return;
        }
        let visible = count > 1;
        self.strip.bar.settle(visible.then_some(count));
        if !visible {
            self.strip.snapshot = None;
            self.strip.drop_zone.set(
                (self.view != View::Dual)
                    .then(|| Bounds::new(Point::default(), window.viewport_size())),
            );
            self.slots.tabs = None;
            self.strip.bounds.set(None);
            return;
        }
        self.strip.bar.width = self.geometry(window).tabs.width;
        let Some(host) = &self.panes[side] else {
            return;
        };
        self.strip.bar.caption = f32::from(host.list.borrow().typeface.caption);
        self.strip.bar.icons = super::icons::wanted(&self.launch.settings.data);
        let frame = self.strip.bar.frame(
            &self.strip.tabs[side],
            // TabBar.qml titles the current tab from the pane's live path, the one the path bar reads.
            host.pane.path(),
            &self.strip.home,
            &self.locale.catalog.tr("tabs.home"),
        );
        let frame = self.mark_tabs(frame, cx);
        let identities = self.strip.tabs[side]
            .tabs
            .as_ref()
            .map(|tabs| tabs.items.iter().map(|tab| tab.identity).collect())
            .unwrap_or_default();
        if self
            .strip
            .snapshot
            .as_ref()
            .is_none_or(|snapshot| snapshot.frame != frame)
        {
            self.strip.frames.borrow_mut().version += 1;
        }
        self.strip.snapshot = Some(super::atomic::Snapshot {
            side,
            frame: frame.clone(),
            identities,
        });
        let view = match &self.strip.view {
            Some(view) => view.clone(),
            None => {
                let shell = cx.entity().downgrade();
                let labels = self.strip.labels.clone();
                let bounds = self.strip.bounds.clone();
                let drop_zone = self.strip.drop_zone.clone();
                let initial = frame.clone();
                let view = cx.new(|_| Strip {
                    frame: initial,
                    labels,
                    bounds,
                    drop_zone,
                    shell,
                    press: None,
                    frames: self.strip.frames.clone(),
                    tip: Default::default(),
                });
                self.strip.view = Some(view.clone());
                view
            }
        };
        view.update(cx, |strip, cx| {
            if strip.frame != frame || font_changed {
                strip.frame = frame;
                cx.notify();
            }
        });
        self.slots.tabs = Some(view.into());
    }

    pub(crate) fn tab_tap(
        &mut self,
        position: Point<Pixels>,
        middle: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tab_tap_hit(self.tab_hit(position), middle, window, cx);
    }

    pub(crate) fn tab_tap_hit(
        &mut self,
        hit: Hit,
        middle: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.strip.bar.drag_from.is_some() || self.strip.bar.out_active {
            return;
        }
        let side = self.focus_side;
        self.with_tabs(|_, panes| {
            panes.visit(side, &mut |tabs, pane| match (hit, middle) {
                (Hit::Tab(index) | Hit::Close(index), true) | (Hit::Close(index), false) => {
                    tabs.close_at(pane, index)
                }
                (Hit::Tab(index), false) => tabs.select_at(pane, index),
                (Hit::New, false) => tabs.open_new(pane, None),
                _ => {}
            });
        });
        self.tabs_changed(window, cx);
    }

    pub(crate) fn end_tab_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.resolve_held_tab();
        let now = self.tab_now();
        if self.strip.bar.out_active {
            let from = self.strip.bar.drag_from;
            let at = self.strip.bar.drop_at;
            self.strip.bar.cancel_out(now);
            self.strip.bar.clear_ack();
            self.strip.bar.drag_from = from;
            self.strip.bar.drop_at = at;
        }
        self.with_tabs(|bar, panes| bar.tab_lift_ended(panes, now));
        self.tabs_changed(window, cx);
    }

    pub(crate) fn tab_native_payload(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<gpui::ExternalDragPayload> {
        let bar = &mut self.strip.bar;
        let token = bar.out_token.clone();
        let live = bar.drag_from.is_some() && transfer::lift_for(&mut bar.lifts, &token).is_some();
        if !live {
            return None;
        }
        let started = self.with_tabs(|bar, panes| bar.tab_lift_moved(panes, 0., true));
        if !started {
            self.publish_tab_notices(cx);
            return None;
        }
        self.strip.capacity.set(true);
        self.strip.drag_serial += 1;
        self.strip.catchers.open(&self.strip.bar.out_payload, cx);
        super::trace::stage("drag-start", || {
            let bar = &self.strip.bar;
            let index = bar.out_index.map_or(-1, |index| index as i64);
            format!(
                "index={index} path={} mime={}",
                bar.out_path,
                transfer::TAB_MIME
            )
        });
        Some(gpui::ExternalDragPayload::Mime(gpui::NativeDragPayload {
            data: gpui::MimePayload::new(
                [gpui::MimeRepresentation::new(
                    transfer::TAB_MIME,
                    self.strip.bar.out_payload.as_bytes(),
                )],
                gpui::SelectionIntent::Copy,
            ),
            actions: gpui::NativeDragActions {
                copy: false,
                move_: true,
                preferred: gpui::NativeDragAction::Move,
            },
            source_id: self.strip.drag_serial,
        }))
    }
}
