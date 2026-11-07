//! The keys and the highest layer the open card or flash view holds.
use super::*;
use gpui::px;

impl Shell {
    /// The open card or the flash view holds every key; Escape and the card's own keys answer, the rest stop here.
    pub(crate) fn archive_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.operations.archive.showing() {
            return false;
        }
        // A pane cover, not a modal: the rail or the other dual pane keeps its keys while it holds focus.
        if !self.operations.archive.card_open() {
            if !self.flash_has_keys(window, cx) {
                return false;
            }
            if keys::key_of(event) == Some(keys::Key::Tab) && self.flash_to_rail(window, cx) {
                cx.stop_propagation();
                return true;
            }
        }
        if self.dialog_key(event, cx) {
            return true;
        }
        cx.stop_propagation();
        let Some(key) = keys::key_of(event).map(|key| key.read(window.layout_mirrored())) else {
            return true;
        };
        let catalog = self.locale.catalog.clone();
        let archive = &mut self.operations.archive;
        let (side, effects) =
            if let Some(card) = archive.compress.as_mut().filter(|card| card.opened()) {
                (archive.card_side, card.key(&catalog, key))
            } else if archive.extract.opened() {
                (archive.card_side, archive.extract.key(&catalog, key))
            } else {
                (archive.flash_side, archive.flash.key(key))
            };
        self.archive_effects(side, effects, window, cx);
        true
    }

    /// The cards over a scrim, or the flash view over its pane's slot, drawn on the highest layer while showing.
    pub(crate) fn archive_layer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.operations.archive.showing() {
            return None;
        }
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = crate::list::Palette::inherited(&window.text_style(), theme);
        let typeface = self.active_typeface();
        let catalog = self.locale.catalog.clone();
        let keyed = self.flash_has_keys(window, cx);
        let covered = self.flash_covers(self.operations.archive.flash_side);
        let room = f32::from(window.viewport_size().width);
        let radius = self.corner;
        self.sync_dialog_editor();
        self.rename.ime.palette = Some(crate::g2::op1::palette::CardPalette::from_list(
            &palette, radius,
        ));
        self.rename.ime.error_ink = false;
        let archive = &mut self.operations.archive;
        if archive.card_open() {
            let mut card = match archive.compress.as_ref().filter(|card| card.opened()) {
                Some(card) => card.card(&catalog),
                None => archive.extract.card(&catalog),
            };
            for block in &mut card.body {
                if let super::super::cards::Block::Field(field) = block {
                    if field.focused {
                        field.input = Some(cx.entity());
                    }
                }
            }
            let eyes = [archive.eye.clone(), archive.confirm_eye.clone()];
            let press = cx.listener(|shell: &mut Shell, at: &usize, window, cx| {
                let catalog = shell.locale.catalog.clone();
                let archive = &mut shell.operations.archive;
                let side = archive.card_side;
                let effects = match archive.compress.as_mut().filter(|card| card.opened()) {
                    Some(card) => card.press_at(&catalog, *at),
                    None => archive.extract.press_at(&catalog, *at),
                };
                shell.archive_effects(side, effects, window, cx);
            });
            let drawn = StandIn.present_probed(
                &card,
                &palette,
                &typeface,
                room,
                radius,
                &eyes,
                Some(Rc::new(press)),
            );
            let ground = cx.listener(|shell: &mut Shell, _: &MouseDownEvent, window, cx| {
                let archive = &mut shell.operations.archive;
                let side = archive.card_side;
                let effects = match archive.compress.as_mut().filter(|card| card.opened()) {
                    Some(card) => card.press(&shell.locale.catalog.clone(), compress::Stop::Cancel),
                    None => archive.extract.press(
                        &shell.locale.catalog.clone(),
                        super::super::extract::Stop::Cancel,
                    ),
                };
                shell.archive_effects(side, effects, window, cx);
            });
            // DialogField's pressed state: a press held on a field's eye reveals that field alone until any release.
            let probes = eyes;
            // Capture runs before the card frame keeps the press, so the eye still sees a press the field answers too.
            let eye = cx.listener(move |shell: &mut Shell, event: &MouseDownEvent, _, cx| {
                if event.button != MouseButton::Left {
                    return;
                }
                let archive = &mut shell.operations.archive;
                let on = |at: usize| {
                    probes[at]
                        .get()
                        .is_some_and(|eye| eye.contains(&event.position))
                };
                match archive.compress.as_mut().filter(|card| card.opened()) {
                    Some(card) if on(0) => card.eye_press(compress::Stop::Password),
                    Some(card) if on(1) => card.eye_press(compress::Stop::Confirm),
                    None if on(0) => archive.extract.eye_press(),
                    _ => return,
                }
                cx.notify();
            });
            let release = || {
                cx.listener(|shell: &mut Shell, _: &MouseUpEvent, _, cx| {
                    let archive = &mut shell.operations.archive;
                    let held = archive.compress.as_mut().filter(|card| card.eye_held());
                    if !archive.extract.eye_held() && held.is_none() {
                        return;
                    }
                    archive.extract.eye_release();
                    if let Some(card) = held {
                        card.eye_release(compress::Stop::Password);
                        card.eye_release(compress::Stop::Confirm);
                    }
                    cx.notify();
                })
            };
            return Some(
                div()
                    .id("op2-scrim")
                    .debug_selector(|| "op2-scrim".into())
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .occlude()
                    .bg(palette
                        .background
                        .opacity(flea::ui_logic_op1::card::scrim_alpha(0)))
                    .on_mouse_down(MouseButton::Left, ground)
                    .capture_any_mouse_up(release())
                    // A release dragged off the window misses the scrim's hitbox, so it masks here.
                    .on_mouse_up_out(MouseButton::Left, release())
                    .child(div().size_full().capture_any_mouse_down(eye).child(drawn))
                    .into_any_element(),
            );
        }
        // FlashHost.qml rides its pane: a flash left on the second pane stays open, unseen, while one pane shows.
        if !covered {
            return None;
        }
        let mut view = archive.flash.view(&catalog);
        view.focused &= keyed;
        let drives = (0..view.drives.len())
            .map(|at| {
                Rc::new(
                    cx.listener(move |shell: &mut Shell, _: &ClickEvent, window, cx| {
                        let effects = shell.operations.archive.flash.choose_drive(at);
                        shell.archive_effects(
                            shell.operations.archive.flash_side,
                            effects,
                            window,
                            cx,
                        );
                    }),
                ) as ClickTap
            })
            .collect();
        let back: ClickTap = Rc::new(cx.listener(
            |shell: &mut Shell, _: &ClickEvent, window, cx| {
                let effects = shell.operations.archive.flash.close();
                shell.archive_effects(shell.operations.archive.flash_side, effects, window, cx);
            },
        ));
        let image: ClickTap = Rc::new(cx.listener(
            |shell: &mut Shell, _: &ClickEvent, window, cx| {
                let effects = shell.operations.archive.flash.choose_image();
                shell.archive_effects(shell.operations.archive.flash_side, effects, window, cx);
            },
        ));
        let step: StepTap = Rc::new(cx.listener(
            |shell: &mut Shell, _: &MouseUpEvent, window, cx| {
                let effects = shell.operations.archive.flash.press_step();
                shell.archive_effects(shell.operations.archive.flash_side, effects, window, cx);
            },
        ));
        let hover: HoverTap = Rc::new(cx.listener(|shell: &mut Shell, over: &bool, _, cx| {
            shell
                .operations
                .archive
                .flash
                .set_action_hover(*over, cx.background_executor().now());
            cx.notify();
        }));
        let down: DownTap = Rc::new(cx.listener(|shell: &mut Shell, _: &MouseDownEvent, _, cx| {
            shell
                .operations
                .archive
                .flash
                .set_action_pressed(true, cx.background_executor().now());
            cx.notify();
        }));
        let up: UpTap = Rc::new(cx.listener(|shell: &mut Shell, _: &MouseUpEvent, _, cx| {
            shell
                .operations
                .archive
                .flash
                .set_action_pressed(false, cx.background_executor().now());
            cx.notify();
        }));
        let released = cx.listener(|shell: &mut Shell, _: &(), _, cx| {
            shell
                .operations
                .archive
                .flash
                .set_action_pressed(false, cx.background_executor().now());
            cx.notify();
        });
        let cancel: CancelTap = Rc::new(move |window, cx| released(&(), window, cx));
        let taps = FlashTaps {
            back: Some(back),
            image: Some(image),
            drives,
            step: Some(step),
            hover: Some(hover),
            down: Some(down),
            up: Some(up),
            cancel: Some(cancel),
            action_probe: Some(self.operations.archive.flash_action.clone()),
        };
        // A hidden step action paints nothing, so the last frame's action is forgotten first.
        self.operations.archive.flash_action.set(None);
        // The flash covers its pane's slot as Trash does, so the rail and the status bar stay drawn.
        let slot = self.geometry(window).panes[self.operations.archive.flash_side];
        let archive = &mut self.operations.archive;
        let now = cx.background_executor().now();
        let reduced = cx.reduce_motion();
        let press = *archive.flash.press_anim(now);
        if press.at(now) != press.to {
            window.request_animation_frame();
        }
        let drawn = StandIn.present_flash(FlashFrame {
            view: &view,
            i18n: &catalog,
            palette: &palette,
            chrome: theme.chrome(),
            typeface: &typeface,
            radius,
            taps: &taps,
            press: &press,
            now,
            reduced,
            window,
        });
        Some(
            div()
                .id("op2-flash-layer")
                .debug_selector(|| "op2-flash-layer".into())
                .absolute()
                .left(px(slot.x))
                .top(px(slot.y))
                .w(px(slot.width))
                .h(px(slot.height))
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|shell: &mut Shell, _: &MouseDownEvent, window, cx| {
                        shell.flash_take_keys(window, cx)
                    }),
                )
                .child(drawn)
                .into_any_element(),
        )
    }
}
