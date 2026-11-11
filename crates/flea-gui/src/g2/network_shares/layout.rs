//! Board geometry and content assembled only when the network card changes.
use super::drawing::{field, BoxPaint, Drawn, Rect, WIDTH};
use super::{dialog::Focus, view::NetworkView};
use gpui::{px, TextRun, Window};

impl Drawn {
    pub fn build(view: &NetworkView, width: f32, height: f32, window: &Window) -> Self {
        let p = view.palette;
        let d = &view.model;
        let w = &view.words;
        let card_w = WIDTH.min((width - 16.).max(0.));
        let card_h = view.frame_height(height);
        let body_h = (card_h - view.footer_height()).max(0.);
        let mut out = Self {
            tracking: w.tracking,
            revision: view.revision,
            body_height: body_h,
            origin: ((width - card_w) / 2., (height - card_h) / 2.),
            card: Some(Rect {
                x: 0.,
                y: 0.,
                w: card_w,
                h: card_h,
            }),
            ..Self::default()
        };
        out.boxes.push(BoxPaint {
            field: None,
            rect: out.card.unwrap(),
            fill: p.surface,
            border: p.muted_frame,
            radius: p.radius,
            border_width: 1.,
            clip: None,
        });
        out.label(
            if d.failed_connect {
                &w.failed_title
            } else if d.retry {
                &w.retry_title
            } else {
                &w.title
            },
            Rect {
                x: 17.,
                y: 17.,
                w: 420.,
                h: 20.,
            },
            p.foreground,
            16.,
            true,
            window,
        );
        out.label(
            &w.escape,
            Rect {
                x: 460.,
                y: 17.,
                w: 24.,
                h: 20.,
            },
            p.muted,
            12.,
            false,
            window,
        );
        out.boxes.push(BoxPaint {
            field: None,
            rect: Rect {
                x: 17.,
                y: 48.,
                w: 465.,
                h: 1.,
            },
            fill: p.muted_frame.opacity(0.4),
            border: p.muted_frame.opacity(0.),
            radius: 0.,
            border_width: 1.,
            clip: None,
        });
        let body_start = (out.text.len(), out.boxes.len(), out.hits.len());
        let mut x = 17.;
        for (index, label) in w.protocols.iter().enumerate() {
            let measured = window.text_system().layout_line(
                label,
                px(14.),
                &[TextRun {
                    len: label.len(),
                    font: window.text_style().font(),
                    color: p.foreground,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            );
            let cw = f32::from(measured.width) + 20.;
            out.control(
                Focus::Protocol(index),
                Rect {
                    x,
                    y: 61.,
                    w: cw,
                    h: 30.,
                },
                index == d.form.protocol as usize,
                view,
            );
            out.label(
                label,
                Rect {
                    x: x + 10.,
                    y: 61.,
                    w: cw - 20.,
                    h: 30.,
                },
                if index == d.form.protocol as usize {
                    p.foreground
                } else {
                    p.muted
                },
                14.,
                false,
                window,
            );
            x += cw + 4.;
        }
        let derived_name = view.derived_name();
        let fields = [
            (Focus::Label, 0, &d.form.label),
            (Focus::Host, 1, &d.form.host),
            (Focus::Port, 2, &d.form.port),
            (Focus::Path, 3, &d.form.path),
            (Focus::Domain, 4, &d.form.domain),
            (Focus::User, 5, &d.form.user),
        ];
        for (focus, index, value) in fields {
            if !d.ring().contains(&focus) {
                continue;
            }
            let mut rect = field(focus).unwrap();
            if d.form.protocol != flea::ui_logic_network_shares::form::Protocol::Smb
                && focus == Focus::User
            {
                rect.y -= 65.;
            }
            let label = if focus == Focus::Path {
                match d.form.protocol {
                    flea::ui_logic_network_shares::form::Protocol::Nfs => &w.export,
                    flea::ui_logic_network_shares::form::Protocol::Smb => &w.fields[index],
                    _ => &w.path,
                }
            } else {
                &w.fields[index]
            };
            out.label(
                label,
                Rect {
                    y: rect.y - 19.,
                    h: 14.,
                    ..rect
                },
                p.muted,
                12.,
                false,
                window,
            );
            out.control(focus, rect, false, view);
            let (text, ink) =
                if value.is_empty() && focus == Focus::Label && d.focus != Focus::Label {
                    (&derived_name, p.muted)
                } else if value.is_empty() && focus == Focus::Host {
                    (&w.host_hint, p.muted)
                } else {
                    (value, p.foreground)
                };
            out.label(
                text,
                Rect {
                    x: rect.x + 10.,
                    w: rect.w - 20.,
                    ..rect
                },
                ink,
                14.,
                false,
                window,
            );
            out.caret(focus, rect, view);
        }
        if d.form.protocol.credentials() {
            let mut rect = field(Focus::Password).unwrap();
            if d.form.protocol != flea::ui_logic_network_shares::form::Protocol::Smb {
                rect.y -= 65.;
            }
            out.label(
                &w.fields[6],
                Rect {
                    y: rect.y - 19.,
                    h: 14.,
                    ..rect
                },
                p.muted,
                12.,
                false,
                window,
            );
            out.control(Focus::Password, rect, false, view);
            let mask = if d.reveal {
                String::from_utf8_lossy(d.password.expose()).into_owned()
            } else {
                "•".repeat(d.password.chars())
            };
            out.label(
                &mask,
                Rect {
                    x: rect.x + 10.,
                    w: rect.w - 42.,
                    ..rect
                },
                p.foreground,
                14.,
                false,
                window,
            );
            out.caret(Focus::Password, rect, view);
            out.hits.push((
                Focus::Eye,
                Rect {
                    x: 448.,
                    y: rect.y,
                    w: 34.,
                    h: 30.,
                },
            ));
            out.label(
                &w.password_hint,
                Rect {
                    x: 17.,
                    y: rect.y + 42.,
                    w: 465.,
                    h: 18.,
                },
                p.muted,
                12.,
                false,
                window,
            );
        } else {
            out.label(
                &w.nfs_hint,
                Rect {
                    x: 17.,
                    y: 298.,
                    w: 465.,
                    h: 18.,
                },
                p.muted,
                12.,
                false,
                window,
            );
        }
        if d.form.protocol.tls_control() {
            let rect = Rect {
                x: 17.,
                y: 481.,
                w: 465.,
                h: 30.,
            };
            out.hits.push((Focus::Tls, rect));
            out.boxes.push(BoxPaint {
                field: None,
                rect: Rect {
                    x: 17.,
                    y: 487.,
                    w: 18.,
                    h: 18.,
                },
                fill: if d.form.tls { p.foreground } else { p.surface },
                border: p.muted_frame,
                radius: p.radius / 2.,
                border_width: 2.,
                clip: None,
            });
            out.label(
                &w.tls,
                Rect { x: 44., ..rect },
                p.foreground,
                14.,
                false,
                window,
            );
        }
        if let Some(uri) = d.form.uri() {
            let line = w.mounts_as.replace("%1", &uri);
            out.label(
                &line,
                Rect {
                    x: 17.,
                    y: 523.,
                    w: 465.,
                    h: 18.,
                },
                p.muted,
                12.,
                false,
                window,
            );
            // The board draws the address in the foreground after the muted words.
            if let Some(at) = w.mounts_as.find("%1") {
                out.ink_piece(at..at + uri.len(), p.foreground);
            }
        }
        Self::status(&mut out, view, window);
        out.fit_body(body_start, view.scroll, card_w, body_h);
        let action = if d.committed {
            &w.close
        } else if d.saving {
            &w.saving
        } else if d.connecting {
            &w.connecting
        } else if d.retry {
            &w.retry
        } else {
            &w.connect
        };
        let measure = |label: &str| {
            f32::from(
                window
                    .text_system()
                    .layout_line(
                        label,
                        px(14.),
                        &[TextRun {
                            len: label.len(),
                            font: window.text_style().font(),
                            color: p.foreground,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                    )
                    .width,
            ) + 20.
        };
        let action_w = measure(action);
        let cancel_w = measure(&w.cancel);
        let button_y = body_h - 47.;
        let action_rect = Rect {
            x: card_w - 17. - action_w,
            y: button_y,
            w: action_w,
            h: 30.,
        };
        let cancel_rect = Rect {
            x: action_rect.x - 9. - cancel_w,
            y: button_y,
            w: cancel_w,
            h: 30.,
        };
        for (focus, rect, text) in [
            (Focus::Cancel, cancel_rect, &w.cancel),
            (Focus::Connect, action_rect, action),
        ] {
            out.button(focus, rect, text, view, window);
        }
        out.provider_footer(view, button_y, card_w, window);
        out
    }
}
