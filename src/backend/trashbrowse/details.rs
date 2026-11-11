// Bounded multi-location info calls keep a large review off the per-item process path.
use super::*;

const BATCH: usize = 128;
const ATTRIBUTES: &str = "--attributes=standard::size,standard::type,trash::deletion-date,id::file,id::filesystem,standard::icon,standard::target-uri";

pub(super) fn records(manifest: &Manifest, cursor: &mut u64) -> Result<Vec<String>, String> {
    let mut batch = Vec::new();
    for _ in 0..BATCH {
        let Some(bytes) = manifest.records().next(cursor)? else {
            break;
        };
        batch.push(record_text(bytes)?);
    }
    Ok(batch)
}

pub(super) fn many(items: &[Item]) -> Result<Vec<Detail>, Refusal> {
    let provider: Vec<_> = items
        .iter()
        .filter(|item| !item.uri.starts_with(crate::backend::topdirtrash::PREFIX))
        .cloned()
        .collect();
    let mut args = vec!["info", "--nofollow-symlinks", ATTRIBUTES];
    args.extend(provider.iter().map(|item| item.uri.as_str()));
    let mut supplied = if provider.is_empty() {
        Vec::new()
    } else {
        parse(&provider, &gio(&args)?)?
    }
    .into_iter();
    let resolve = crate::backend::topdirtrash::resolver();
    items
        .iter()
        .map(|item| {
            if item.uri.starts_with(crate::backend::topdirtrash::PREFIX) {
                super::topdir::detail_with(&resolve, item)
            } else {
                supplied
                    .next()
                    .ok_or_else(|| unreadable("The Trash helper returned incomplete item details."))
            }
        })
        .collect()
}

// Sample input: "type: regular\nsize:  1\nuri: trash:///a\nattributes:\n  standard::size: 1\n  id::file: a\n  id::filesystem: fs\n".
fn parse(items: &[Item], text: &str) -> Result<Vec<Detail>, Refusal> {
    let mut sections = HashMap::new();
    let mut current = None;
    let mut body = String::new();
    for line in text.lines() {
        if let Some(uri) = line.strip_prefix("uri: ") {
            if let Some(previous) = current.replace(uri.to_string()) {
                if sections
                    .insert(previous, std::mem::take(&mut body))
                    .is_some()
                {
                    return Err(unreadable(
                        "The Trash helper returned duplicate item details.",
                    ));
                }
            }
        } else if current.is_some() {
            body.push_str(line);
            body.push('\n');
        }
    }
    if let Some(uri) = current {
        if sections.insert(uri, body).is_some() {
            return Err(unreadable(
                "The Trash helper returned duplicate item details.",
            ));
        }
    }
    if sections.len() != items.len() {
        return Err(unreadable(
            "The Trash helper returned incomplete item details.",
        ));
    }
    items
        .iter()
        .map(|item| {
            let text = sections.remove(&item.uri).ok_or_else(|| {
                unreadable("The Trash helper returned a different item identity.")
            })?;
            detail_text(item, &text)
        })
        .collect()
}

// The row member naming the regular file a text preview reads, checked against gio's review as the window is listed; a folder, a changed backing or a name JSON cannot carry names nothing.
pub(super) fn content(item: &Detail) -> String {
    let path = item.backing.as_ref().filter(|_| !item.directory).and_then(Reviewed::thumbnail)
        .and_then(|(path, _)| path.to_str().map(str::to_owned)).unwrap_or_default();
    format!(r#""content":"{}""#, escape(&path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_details_keep_request_order_and_refuse_incomplete_or_ambiguous_answers() {
        let items = listed("trash:///a\t/a\ntrash:///b\t/b\n");
        let a = "uri: trash:///a\n  standard::size: 1\n  id::file: a\n  id::filesystem: fs\n";
        let b = "uri: trash:///b\n  standard::size: 2\n  id::file: b\n  id::filesystem: fs\n";
        let rows = parse(&items, &format!("{b}{a}")).unwrap();
        assert_eq!((rows[0].size, rows[1].size), (1, 2));
        for text in [
            a.to_string(),
            format!("{a}{a}"),
            format!("{a}{}", b.replace("trash:///b", "trash:///c")),
        ] {
            assert!(parse(&items, &text).is_err(), "{text}");
        }
    }

    #[test]
    fn real_gio_headers_keep_namespaced_attributes() {
        let d = crate::backend::testdir::TestDir::new("trash-gio-headers");
        let a = d.file("a", "a");
        let b = d.dir("b");
        let items = [a, b].into_iter().map(|path| Item {
            uri: format!("file://{}", path.display()),
            original: path.to_str().unwrap().into(),
        }).collect::<Vec<_>>();
        let text = gio(&["info", "--nofollow-symlinks", ATTRIBUTES, &items[0].uri, &items[1].uri]).unwrap();
        let rows = parse(&items, &text).unwrap();
        for (item, actual) in items.iter().zip(&rows) {
            let expected = detail(item).unwrap();
            assert_eq!(actual.size, expected.size);
            assert_eq!(actual.directory, expected.directory);
            assert_eq!(actual.identity, expected.identity);
            assert_eq!(actual.icon, expected.icon);
            assert_eq!(actual.deleted, expected.deleted);
        }
        assert!(!rows[0].directory && rows[1].directory);
        assert_eq!(rows[0].size, 1);
        assert_ne!(rows[0].identity, rows[1].identity);
    }
}
