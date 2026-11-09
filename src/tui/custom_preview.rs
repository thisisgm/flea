//! Terminal equivalents of the custom GUI preview: file navigation, facts and a name strip.
use super::{input::Key, model::Model, wire::Wire};
use crate::jsondoc::Json;
use std::io;

pub struct State {
    pub enabled: bool,
    pub info: bool,
    pub strip: bool,
    pub media_controls: bool,
    cursor: Option<usize>,
    muted: bool,
    mute_pending: bool,
    folder: Option<std::path::PathBuf>,
    folder_pending: bool,
    returning: bool,
    navigation: Option<isize>,
}
impl State {
    pub fn new(settings: &Json) -> Self {
        let preview = settings.get("preview");
        let flag = |key| preview.and_then(|p| p.get(key)).and_then(Json::as_bool);
        Self {
            enabled: std::env::var_os("FLEA_CUSTOM_PREVIEW").is_some_and(|v| v == "1"),
            info: flag("infoPanel") == Some(true),
            strip: true,
            media_controls: flag("mediaControls") != Some(false),
            cursor: None,
            muted: false,
            mute_pending: false,
            folder: None,
            folder_pending: false,
            returning: false,
            navigation: None,
        }
    }
}
pub fn begin(m: &mut Model) {
    if m.custom_preview.enabled {
        m.custom_preview.cursor = Some(m.cursor);
    }
}
pub fn open(m: &mut Model, wire: &mut Wire) -> io::Result<()> {
    begin(m);
    if m.custom_preview.enabled && m.rows.get(&m.cursor).is_some_and(|r| r.directory) {
        if let Some(path) = m.current_path() {
            m.custom_preview.folder = Some(m.path.clone());
            m.custom_preview.folder_pending = true;
            m.open(path, wire)?;
        }
    }
    Ok(())
}
pub(super) fn close(m: &mut Model, wire: &mut Wire) -> io::Result<()> {
    m.quicklook = false;
    m.preview_focus = false;
    m.player = None;
    m.custom_preview.navigation = None;
    if m.custom_preview.folder.is_some() {
        m.custom_preview.returning = true;
        return settle(m, wire);
    }
    if let Some(cursor) = m.custom_preview.cursor.take() {
        m.cursor = cursor.min(m.total.saturating_sub(1));
    }
    m.window(wire)
}
pub fn key(m: &mut Model, key: &Key, wire: &mut Wire) -> io::Result<bool> {
    if !m.custom_preview.enabled || !m.quicklook || !key.mods.is_empty() {
        return Ok(false);
    }
    let key = if key.text.is_empty() {
        key.name.as_str()
    } else {
        key.text.as_str()
    };
    match key {
        "m" if m.player.is_some() => {
            m.custom_preview.muted = !m.custom_preview.muted;
            m.custom_preview.mute_pending = true;
        }
        "p" | " " | "Space" if m.player.is_some() => {
            m.player.as_mut().unwrap().toggle()?;
            m.custom_preview.mute_pending = true;
        }
        "h" | "l" if m.player.is_some() => {
            m.player
                .as_mut()
                .unwrap()
                .seek(if key == "h" { -5 } else { 5 })?;
        }
        "i" => {
            m.custom_preview.info = !m.custom_preview.info;
            m.preview_layout_generation = usize::MAX;
        }
        "t" => m.custom_preview.strip = !m.custom_preview.strip,
        "Escape" => {
            close(m, wire)?;
        }
        " " | "Space" if m.player.is_none() => {
            close(m, wire)?;
        }
        "Return" | "Enter" | "\r" => {
            if let Some(path) = m.current_path() {
                if crate::open::open(&path.to_string_lossy()) != 0 {
                    m.fail_key("tui.model.could_not_open_selected_file");
                }
            }
            close(m, wire)?;
        }
        "j" | "k" | "Left" | "Right" | "Up" | "Down" => {
            let step = if matches!(key, "k" | "Left" | "Up") {
                -1
            } else {
                1
            };
            let mut next = m.cursor as isize + step;
            while next >= 0 && (next as usize) < m.total {
                match m.rows.get(&(next as usize)) {
                    Some(row)
                        if row.directory
                            || flea::ui_logic_pv1::kinds::quick_look_kind(
                                &row.icon, &row.name, false,
                            ) == flea::ui_logic_pv1::kinds::State::Unsupported =>
                    {
                        next += step
                    }
                    _ => break,
                }
            }
            if next >= 0 && (next as usize) < m.total {
                m.custom_preview.mute_pending = true;
                m.custom_preview.navigation = Some(step);
                m.cursor = next as usize;
                m.window(wire)?;
                super::preview::load(m, true);
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
pub fn settle(m: &mut Model, wire: &mut Wire) -> io::Result<()> {
    if !m.custom_preview.enabled || m.pending.is_some() {
        return Ok(());
    }
    if m.custom_preview.returning {
        if let Some(path) = m.custom_preview.folder.take() {
            m.restore_cursor = m.custom_preview.cursor.take();
            m.custom_preview.returning = false;
            m.custom_preview.folder_pending = false;
            m.open(path, wire)?;
        }
        return Ok(());
    }
    if !m.quicklook {
        return Ok(());
    }
    if m.custom_preview.folder_pending {
        if let Some((&index, _)) = m.rows.iter().find(|(_, r)| !r.directory) {
            m.cursor = index;
        }
        m.custom_preview.folder_pending = false;
        m.window(wire)?;
    }
    if let Some(step) = m.custom_preview.navigation {
        if let Some(row) = m.rows.get(&m.cursor) {
            if row.directory
                || flea::ui_logic_pv1::kinds::quick_look_kind(&row.icon, &row.name, false)
                    == flea::ui_logic_pv1::kinds::State::Unsupported
            {
                let next = m.cursor as isize + step;
                if next >= 0 && (next as usize) < m.total {
                    m.cursor = next as usize;
                    m.window(wire)?;
                } else {
                    m.custom_preview.navigation = None;
                }
            } else {
                m.custom_preview.navigation = None;
            }
        }
    }
    Ok(())
}
pub fn prepare(m: &mut Model) {
    if !m.custom_preview.enabled || !m.custom_preview.mute_pending {
        return;
    }
    if let Some(player) = m.player.as_mut().filter(|p| !p.at_rest()) {
        let command = vec![
            super::wire::word("set_property"),
            super::wire::word("mute"),
            Json::Bool(m.custom_preview.muted),
        ];
        if player.command(command).is_ok() {
            m.custom_preview.mute_pending = false;
        }
    }
}
pub fn strip(m: &Model, width: usize, theme: &super::theme::Theme) -> String {
    if m.custom_preview.enabled && m.custom_preview.info {
        let info = m
            .rows
            .get(&m.cursor)
            .map(|r| {
                format!(
                    "{} B  mode {:o}  {}",
                    r.size,
                    r.mode & 0o7777,
                    m.row_path(r).display()
                )
            })
            .unwrap_or_default();
        return format!(
            "{}{}",
            theme.meta,
            super::cells::fit(&super::cells::clean(&info), width)
        );
    }
    if !m.custom_preview.enabled || !m.custom_preview.strip {
        return super::cells::fit("", width);
    }
    let files: Vec<_> = m.rows.iter().filter(|(_, r)| !r.directory).collect();
    let current = files.iter().position(|(i, _)| **i == m.cursor).unwrap_or(0);
    let first = current.saturating_sub(2);
    let text = files
        .iter()
        .skip(first)
        .take(5)
        .map(|(i, r)| {
            let name = super::cells::clean(&r.name);
            if **i == m.cursor {
                format!("[{name}]")
            } else {
                name
            }
        })
        .collect::<Vec<_>>()
        .join("  ");
    format!(
        "{}{}",
        theme.meta,
        super::cells::fit(&super::cells::ellipsis(&text, width), width)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::{action_key_tests::listing, wire};
    #[test]
    fn custom_port_tui_navigation_restores_browser_cursor_and_keeps_preview_open() {
        let (mut wire, reader) = wire::echo_wire();
        let mut m = listing(&mut wire, "default");
        let first = m.rows.get(&1).unwrap().clone();
        m.rows.insert(
            2,
            super::super::model::Row {
                name: "next.txt".into(),
                icon: "text-plain".into(),
                ..first
            },
        );
        m.total = 3;
        m.cursor = 1;
        m.custom_preview.enabled = true;
        begin(&mut m);
        m.quicklook = true;
        assert!(key(&mut m, &Key::named("Right", ""), &mut wire).unwrap());
        assert_eq!(m.cursor, 2);
        assert!(m.quicklook);
        key(&mut m, &Key::character('i', ""), &mut wire).unwrap();
        assert!(m.custom_preview.info);
        key(&mut m, &Key::character('t', ""), &mut wire).unwrap();
        assert!(!m.custom_preview.strip);
        key(&mut m, &Key::named("Escape", ""), &mut wire).unwrap();
        assert_eq!(m.cursor, 1);
        assert!(!m.quicklook);
        wire::finish(wire, reader);
    }
}
