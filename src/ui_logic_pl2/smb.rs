//! Match GIO's SMB FUSE identity and resolve ambiguous per-user mounts without reading labels.
use super::{mounts::decode_path, protocols};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parts {
    pub host: String,
    pub share: String,
    pub user: String,
}

pub fn parts(uri: &str) -> Option<Parts> {
    if protocols::scheme_of(uri) != "smb" {
        return None;
    }
    let auth = protocols::authority(uri);
    if auth.is_empty() {
        return None;
    }
    let info = auth.rsplit_once('@').map_or("", |(info, _)| info);
    let user = info.split_once(';').map_or(info, |(_, user)| user);
    let path = &uri[uri.find("://")? + 3 + auth.len()..];
    Some(Parts {
        host: protocols::host_of(uri).to_lowercase(),
        share: decode_path(path.trim_start_matches('/').split('/').next().unwrap_or("")),
        user: decode_path(user),
    })
}

pub fn fuse_parts(path: &str) -> Option<Parts> {
    let text = path.split_once("smb-share:server=")?.1;
    let (host, text) = text.split_once(",share=")?;
    if host.is_empty() || host.contains('/') {
        return None;
    }
    let mut tail = text.split([',', '/']);
    let share = tail.next()?;
    if share.is_empty() {
        return None;
    }
    let user = tail
        .next()
        .and_then(|s| s.strip_prefix("user="))
        .unwrap_or("");
    Some(Parts {
        host: host.to_lowercase(),
        share: share.into(),
        user: user.into(),
    })
}

pub fn matches(uri: &str, path: &str) -> bool {
    let (Some(want), Some(got)) = (parts(uri), fuse_parts(path)) else {
        return true;
    };
    let user = got
        .user
        .split_once(';')
        .map_or(got.user.as_str(), |(_, user)| user);
    want.host == got.host
        && want.share.eq_ignore_ascii_case(&got.share)
        && want.user.eq_ignore_ascii_case(user)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resolved {
    pub path: String,
    pub users: Vec<String>,
}

pub fn resolve(uri: &str, paths: &[String], login: &str) -> Resolved {
    let Some(want) = parts(uri) else {
        return Resolved::default();
    };
    let hits: Vec<_> = paths
        .iter()
        .filter_map(|path| Some((path, fuse_parts(path)?)))
        .filter(|(_, p)| p.host == want.host && p.share.eq_ignore_ascii_case(&want.share))
        .collect();
    let chosen = if !want.user.is_empty() {
        hits.iter()
            .find(|(_, p)| p.user.eq_ignore_ascii_case(&want.user))
    } else if hits.len() == 1 {
        hits.first()
    } else {
        hits.iter()
            .find(|(_, p)| p.user.eq_ignore_ascii_case(login))
    };
    if let Some((path, _)) = chosen {
        return Resolved {
            path: (*path).clone(),
            users: Vec::new(),
        };
    }
    let mut users = Vec::new();
    if want.user.is_empty() {
        for (_, p) in hits {
            if !users.contains(&p.user) {
                users.push(p.user);
            }
        }
    }
    Resolved {
        path: String::new(),
        users,
    }
}
