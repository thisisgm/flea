//! user-dirs.dirs read the way the rail lists it: roles in first-seen order, the last valid path per role.
use super::leaf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserDir {
    pub path: String,
    pub label: String,
    pub label_key: &'static str,
}

// Only a role's standard folder name is product copy; renamed folders retain their own names.
fn label_key(role: &str, label: &str) -> &'static str {
    match (role, label) {
        ("DESKTOP", "Desktop") => "places.row.desktop",
        ("DOCUMENTS", "Documents") => "places.row.documents",
        ("DOWNLOAD", "Downloads") => "places.row.downloads",
        ("MUSIC", "Music") => "places.row.music",
        ("PICTURES", "Pictures") => "places.row.pictures",
        ("VIDEOS", "Videos") => "places.row.videos",
        ("TEMPLATES", "Templates") => "places.row.templates",
        ("PUBLICSHARE", "Public") => "places.row.public",
        // xdg-user-dirs 0.20's user-dirs.defaults lists PROJECTS beside the eight older roles.
        ("PROJECTS", "Projects") => "places.row.projects",
        _ => "",
    }
}

// Sample input: XDG_DOWNLOAD_DIR="$HOME/Photo\"Inbox" # local inbox gives ("DOWNLOAD", Some(".../Photo\"Inbox")).
pub fn parse_line(line: &str, home: &str) -> Option<(String, Option<String>)> {
    let rest = line.trim_start_matches([' ', '\t']).strip_prefix("XDG_")?;
    let cut = rest.find("_DIR")?;
    let role = &rest[..cut];
    if role.is_empty() || !role.bytes().all(|b| b.is_ascii_uppercase()) {
        return None;
    }
    let value = rest[cut + 4..]
        .trim_start_matches([' ', '\t'])
        .strip_prefix('=')?;
    Some((role.to_owned(), quoted_path(value, home)))
}

fn quoted_path(value: &str, home: &str) -> Option<String> {
    let body = value.trim_start_matches([' ', '\t']).strip_prefix('"')?;
    let (from_home, body) = match body.strip_prefix("$HOME") {
        Some(after) if after.starts_with('/') || after.starts_with('"') => (true, after),
        _ => (false, body),
    };
    let mut raw = String::new();
    let mut chars = body.chars();
    loop {
        match chars.next()? {
            '\\' => raw.push(chars.next()?),
            '"' => break,
            c => raw.push(c),
        }
    }
    if !from_home && !body.starts_with('/') {
        return None;
    }
    let path = format!("{}{}", if from_home { home } else { "" }, raw);
    let path = path.trim_end_matches('/');
    path.starts_with('/').then(|| path.to_owned())
}

// Sample input: XDG_DOWNLOAD_DIR="$HOME/Old" then XDG_DOWNLOAD_DIR="$HOME/Inbox"; the last valid path wins.
pub fn read_roles(body: &str, home: &str) -> Vec<(String, String)> {
    let mut roles: Vec<String> = Vec::new();
    let mut paths: Vec<(String, String)> = Vec::new();
    for (role, path) in body.split('\n').filter_map(|line| parse_line(line, home)) {
        if !roles.contains(&role) {
            roles.push(role.clone());
        }
        if let Some(path) = path {
            paths.retain(|(own, _)| *own != role);
            paths.push((role, path));
        }
    }
    roles
        .into_iter()
        .filter_map(|role| paths.iter().find(|(own, _)| *own == role).cloned())
        .collect()
}

// Sample input: XDG_DOWNLOAD_DIR="$HOME/Downloads"; a disabled role that points at Home is no place of its own.
pub fn user_dirs(body: &str, home: &str) -> Vec<UserDir> {
    read_roles(body, home)
        .into_iter()
        .filter(|(_, path)| path != home)
        .map(|(role, path)| UserDir {
            label_key: label_key(&role, leaf(&path)),
            label: leaf(&path).to_owned(),
            path,
        })
        .collect()
}

#[cfg(test)]
#[path = "userdirs_language_tests.rs"]
mod language_tests;
