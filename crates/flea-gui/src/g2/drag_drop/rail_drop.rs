//! A row drag held over or released on a rail place (RailDrop.qml, RailTrashRow.qml), aimed too at a pane's folder rows (RowDropTarget.qml) and the tabs (DropInto.qml): a place takes the transfer a folder row takes, Trash takes dd's trash request.
use super::*;
use flea::ui_logic_drag_drop::{drop_label, line_to, rail_destination, PasteAsk, RailKind, Verb};
use flea::ui_logic_rail_places::rail::Kind;
use flea::ui_logic_trash_browser::dropping;

// The status line's activity owner for the held drag's line (FileDrag.qml dragFeedbackOwner).
pub(super) const DRAG_LINE: u64 = 1 << 33;

#[path = "rail_reaim.rs"]
mod reaim;
#[path = "rail_rest_tab.rs"]
mod rest_tab;

/// RailDrop.js destination: Recent, a tag and every phone row (MTP, PTP, AFC) are never a drop folder.
pub(crate) fn rail_kind(kind: Kind) -> RailKind {
    match kind {
        Kind::Recent | Kind::Tag | Kind::Phone => RailKind::Other,
        _ => RailKind::Folder,
    }
}

/// Where a rail row sends a drop: a place's folder, or the Trash.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Place {
    Folder(String),
    Trash,
}

/// What a held drag rests on: a rail row, a folder row of the pane, or a tab of the strip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Spot {
    Rail(usize),
    Row(usize),
    Tab(usize),
}

/// The spot a held drag rests on, the pane that receives it, and the disk facts once read.
pub(super) struct Aim {
    spot: Spot,
    side: usize,
    name: String,
    place: Place,
    facts: Option<Facts>,
    serial: u64,
    // The caption a folder row draws while the drag rests on it, once the verb is known.
    label: Option<gpui::SharedString>,
}

// Drag.js feedbackLine with no name: the lift's own device stands in for the folder's, so a plain drag reads move.
fn held_line(catalog: &flea::i18n::I18n, count: usize, ctrl: bool, shift: bool) -> String {
    let items = catalog.trn("items.count", count as i64);
    let key = match (ctrl, shift) {
        (true, true) => core::LINK_TO_FOLDER,
        (true, false) => core::COPY_TO_FOLDER,
        _ => core::MOVE_TO_FOLDER,
    };
    catalog.tr_args(key, &[&items])
}

impl Shell {
    /// Hands the rail's rows this window's drop receiver; the rows call it with no lease held.
    pub(crate) fn receive_rail_drops(&mut self, cx: &mut Context<Self>) {
        let Some(join) = &self.rail else {
            return;
        };
        let shell = cx.entity().downgrade();
        join.places.update(cx, |producer, _| {
            producer.drop_sink = Some(Rc::new(move |window, cx| {
                let _ = shell.update(cx, |shell, cx| shell.land_rail_drag(window, cx));
            }));
        });
    }

    // RailDrop.destination and TrashDrop.accepts: the row under point, its pane and where it sends this lift.
    fn rail_target(
        &self,
        point: Point<gpui::Pixels>,
        lift: &Lift,
        cx: &App,
    ) -> Option<(usize, usize, String, Place)> {
        let join = self.rail.as_ref()?;
        let rail = &join.host.read(cx).rail;
        if !rail.shown() || !rail.drawn(cx.background_executor().now()) {
            return None;
        }
        let producer = join.places.read(cx);
        let row = producer.row_at(point)?;
        let entry = producer.entry(row)?;
        let name = producer.laid.borrow().row(row)?.label.clone();
        let side = producer.side;
        let host = self.panes.get(side)?.as_ref()?;
        if host.pane.busy() || entry.invalid_record() {
            return None;
        }
        let paths = lift.paths.as_ref()?;
        let place = if entry.kind == Kind::Trash {
            flea::ui_logic_device_rail::mounts::trashable(&lift.marker.source, None)
                .then_some(Place::Trash)?
        } else {
            let kind = rail_kind(entry.kind);
            let urls: Vec<String> = paths.iter().map(|path| core::uri_for(path)).collect();
            let dest = rail_destination(
                kind,
                &entry.path,
                entry.mounted != Some(false),
                &urls,
                None,
                true,
            );
            (!dest.is_empty()).then_some(Place::Folder(dest))?
        };
        Some((row, side, name, place))
    }

    // DropInto.qml on a tab, then RowDropTarget.qml on a folder row the drag does not carry.
    fn pane_target(
        &self,
        point: Point<gpui::Pixels>,
        lift: &Lift,
    ) -> Option<(Spot, usize, String, Place)> {
        if let Some(tab) = self.file_tab_destination(point) {
            let index = self.strip.tabs[tab.side].resolve(tab.identity)?;
            return (!tab.path.is_empty()).then(|| {
                (
                    Spot::Tab(index),
                    tab.side,
                    tab.path.clone(),
                    Place::Folder(tab.path),
                )
            });
        }
        let side = (0..self.panes.len()).find(|side| {
            self.panes[*side]
                .as_ref()
                .and_then(|host| host.input.frame)
                .is_some_and(|frame| frame.bounds.contains(&point))
        })?;
        let host = self.panes[side].as_ref()?;
        if host.pane.busy()
            || !matches!(
                host.drawn_view,
                Some(
                    crate::window::startup::View::List
                        | crate::window::startup::View::Dual
                        | crate::window::startup::View::Grid
                )
            )
        {
            return None;
        }
        let row = host.input.hit(point)?;
        let own = lift.side == side && lift.listing == host.pane.held.listing;
        if own && lift.rows.contains(&row) {
            return None;
        }
        let name = host
            .pane
            .held
            .row(row)
            .filter(|facts| folder_row(facts))?
            .name
            .clone();
        let dest = row_spot(host, Some(row), &[]);
        Some((Spot::Row(row), side, name, Place::Folder(dest)))
    }

    /// Every move of a held row drag reaches this listener wherever the pointer is, so one pane carries it.
    pub(crate) fn aim_rail_on_drag_move(
        div: gpui::Stateful<gpui::Div>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        gpui::InteractiveElement::on_drag_move::<Seed>(
            div,
            cx.listener(|shell, event: &gpui::DragMoveEvent<Seed>, window, cx| {
                shell.aim_rail_drag(event.event.position, window, cx);
            }),
        )
    }

    /// The pointer moved with this window's row drag held: light the rail row it rests on and say what a release does.
    pub(crate) fn aim_rail_drag(
        &mut self,
        point: Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = window_key(window);
        if let Some(lift) = cx.default_global::<Drags>().lifts.get_mut(&key) {
            lift.left = lift.travelled(at(point));
        }
        let target = cx
            .try_global::<Drags>()
            .and_then(|drags| drags.lifts.get(&key))
            .and_then(|lift| {
                self.rail_target(point, lift, cx)
                    .map(|(row, side, name, place)| (Spot::Rail(row), side, name, place))
                    .or_else(|| self.pane_target(point, lift))
            });
        let Some((spot, side, name, place)) = target else {
            self.clear_rail_aim(window, cx);
            self.say_held_drag(window, cx);
            // Rail and file cues share one line, so a parent crumb's cue says it again with the crumb's name.
            self.refresh_file_cue(window, cx);
            return;
        };
        let drags = cx.default_global::<Drags>();
        if let Some(aim) = drags.aims.get(&key) {
            if aim.spot == spot && aim.side == side && aim.place == place {
                self.say_rail_aim(window, cx);
                return;
            }
        }
        drags.serial += 1;
        let serial = drags.serial;
        let folder = match &place {
            Place::Folder(dest) => Some(dest.clone()),
            Place::Trash => None,
        };
        let sources = drags
            .lifts
            .get(&key)
            .and_then(|lift| lift.paths.clone())
            .unwrap_or_default();
        drags.aims.insert(
            key,
            Aim {
                spot,
                side,
                name,
                place,
                facts: None,
                serial,
                label: None,
            },
        );
        self.rest_on_tab(spot, window, cx);
        self.say_rail_aim(window, cx);
        // RailDrop.qml asks for the place's device before the verb is said; a later aim supersedes this answer.
        if let Some(dest) = folder {
            let facts = cx
                .background_executor()
                .spawn(async move { facts::read(&sources, &dest) });
            cx.spawn_in(window, async move |this, cx| {
                let facts = facts.await;
                let _ = this.update_in(cx, |shell, window, cx| {
                    let key = window_key(window);
                    let Some(aim) = cx.default_global::<Drags>().aims.get_mut(&key) else {
                        return;
                    };
                    if aim.serial == serial {
                        aim.facts = Some(facts);
                        shell.say_rail_aim(window, cx);
                    }
                });
            })
            .detach();
        }
    }

    // FileDrag.showTarget: the row's caption and the status line, from the keys held now; a folder waits for its facts.
    fn say_rail_aim(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = window_key(window);
        let modifiers = window.modifiers();
        let catalog = self.locale.catalog.clone();
        let Some(drags) = cx.try_global::<Drags>() else {
            return;
        };
        let (Some(aim), Some(lift)) = (drags.aims.get(&key), drags.lifts.get(&key)) else {
            return;
        };
        let count = lift.rows.len();
        let said = match &aim.place {
            Place::Trash => Some((
                drop_label(&catalog, Verb::Move),
                dropping::line(
                    &catalog,
                    Some(&dropping::Request::Paths(
                        lift.paths.clone().unwrap_or_default(),
                    )),
                    "",
                ),
            )),
            Place::Folder(dest) => aim.facts.and_then(|facts| {
                let held = Dropping::own(lift, modifiers.control, modifiers.shift);
                let verb = match decide(&held, dest, facts) {
                    Landing::Ask(PasteAsk::Link { .. }) => Verb::Link,
                    Landing::Ask(PasteAsk::Transfer { op, .. }) if op == "move" => Verb::Move,
                    Landing::Ask(PasteAsk::Transfer { .. }) => Verb::Copy,
                    _ => return None,
                };
                Some((
                    drop_label(&catalog, verb),
                    line_to(&catalog, count, &aim.name, verb),
                ))
            }),
        };
        let spot = aim.spot;
        // A folder still waiting for its facts, or one a link leads back to the sources' own, keeps the unnamed line.
        let unnamed = matches!(aim.place, Place::Folder(_))
            && aim.facts.is_none_or(|facts| facts.refused.is_some());
        if let Some(aim) = cx.global_mut::<Drags>().aims.get_mut(&key) {
            aim.label = said.as_ref().map(|(label, _)| label.clone().into());
        }
        let rail = match spot {
            Spot::Rail(row) => Some(row),
            Spot::Row(_) | Spot::Tab(_) => None,
        };
        if unnamed {
            return self.say_held_drag(window, cx);
        }
        self.show_rail_cue(key, rail, said, cx);
    }

    /// FileDrag.leaveTarget: this window's own drag off every target says its line with no folder name.
    pub(crate) fn say_held_drag(&mut self, window: &Window, cx: &mut Context<Self>) {
        let count = cx
            .try_global::<Drags>()
            .and_then(|drags| drags.lifts.get(&window_key(window)))
            .map(|lift| lift.rows.len());
        if let Some(count) = count {
            let modifiers = window.modifiers();
            let line = held_line(
                &self.locale.catalog,
                count,
                modifiers.control,
                modifiers.shift,
            );
            self.show_rail_cue(window_key(window), None, Some((String::new(), line)), cx);
        }
    }

    /// The drag left the rail or ended: no row stays lit and the drag's line leaves the status bar.
    pub(crate) fn clear_rail_aim(&mut self, window: &Window, cx: &mut Context<Self>) {
        let key = window_key(window);
        let held = cx
            .try_global::<Drags>()
            .is_some_and(|drags| drags.aims.contains_key(&key));
        if held {
            cx.global_mut::<Drags>().aims.remove(&key);
            self.strip.bar.hover_exited();
        }
        let said = self.status_messages(cx, |messages, _| {
            messages
                .activities()
                .iter()
                .any(|entry| entry.owner == DRAG_LINE)
        });
        if held || said {
            self.show_rail_cue(key, None, None, cx);
        }
    }

    /// A frame with no drag in flight drops a cue the release did not reach, as a release over the chrome.
    pub(crate) fn settle_rail_drag(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.settle_file_feedback(window, cx);
        if !cx.has_active_drag() {
            self.clear_file_rest(window, cx);
            self.settle_row_drag(window, cx);
        }
    }

    pub(super) fn show_rail_cue(
        &mut self,
        key: u64,
        rail: Option<usize>,
        cue: Option<(String, String)>,
        cx: &mut Context<Self>,
    ) {
        if let Some(join) = &self.rail {
            let target = rail
                .zip(cue.as_ref())
                .map(|(row, (label, _))| (row, gpui::SharedString::from(label.clone())));
            join.places.update(cx, |producer, cx| {
                let mut model = producer.model.borrow_mut();
                if model.drop_target != target {
                    model.drop_target = target;
                    drop(model);
                    cx.notify();
                }
            });
        }
        let line = cue.map(|(_, line)| line).unwrap_or_default();
        let published = self.status_messages(cx, |messages, now| {
            // FileDrag.say: a running transfer keeps the bar; an empty line always leaves.
            let publish =
                line.is_empty() || !messages.activities().iter().any(|entry| entry.running);
            if publish {
                messages.set_activity(DRAG_LINE, &line, 0, false, false, now);
            }
            publish
        });
        // The file cue shares this owner, so the line it said no longer stands once the rail writes.
        if published {
            cx.default_global::<Drags>().said.remove(&key);
        }
        cx.notify();
    }

    /// This window's row drag was released on the rail: the row under the pointer takes it, or nothing happens.
    pub(crate) fn land_rail_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = window_key(window);
        let point = window.mouse_position();
        self.clear_rail_aim(window, cx);
        let Some(lift) = cx.default_global::<Drags>().take(key) else {
            return;
        };
        if !lift.travelled(at(point)) {
            return;
        }
        let Some((_, side, _, place)) = self.rail_target(point, &lift, cx) else {
            return;
        };
        let modifiers = window.modifiers();
        let then = match place {
            Place::Trash => super::wide::Then::Trash,
            Place::Folder(dest) => super::wide::Then::Into(dest),
        };
        let owned = Owned::Own {
            lift,
            ctrl: modifiers.control,
            shift: modifiers.shift,
        };
        self.land_then(side, then, owned, window, cx);
    }

    /// A rail Trash drop of a resolved lift.
    pub(super) fn trash_lift(&mut self, lift: &Lift, cx: &mut Context<Self>) {
        let Some(paths) = lift.paths.clone() else {
            return;
        };
        let line = dropping::Request::Paths(paths).line();
        // dd's cursor anchor only while the source pane still lists the rows it lifted.
        let current = self.panes[lift.side]
            .as_ref()
            .is_some_and(|host| host.pane.held.listing == lift.listing);
        self.trash_dropped(lift.side, line, &lift.rows, current, cx);
    }
}
