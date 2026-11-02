use super::*;
use std::os::unix::net::UnixStream;

#[test]
fn paste_image_browser_and_screenshot_offers_keep_png_availability() {
    for types in [
        vec![
            "text/html",
            "image/png",
            "chromium/x-internal-source-rfh-token",
            "chromium/x-source-url",
        ],
        vec!["text/html", "image/png", "chromium/x-web-custom-data"],
        vec!["text/html", "image/png"],
        vec!["image/png"],
    ] {
        let (stream, _peer) = UnixStream::pair().unwrap();
        let mut conn = Conn::over(stream.into());
        let types = types.into_iter().map(str::to_owned).collect::<Vec<_>>();
        let got = read_offer_with(&mut conn, 10, &types, |_, _, mime| {
            panic!("image availability must not read {mime}")
        })
        .unwrap();
        assert_eq!(got.op, "image", "offer: {types:?}");
        assert!(got.paths.is_empty());
    }
}

#[test]
fn paste_image_files_keep_priority_and_text_is_not_an_image() {
    for (types, expected, paths) in [
        (vec!["text/html", "text/plain"], "none", Vec::new()),
        (
            vec!["text/uri-list", "image/png"],
            "copy",
            vec!["/files/a".to_owned()],
        ),
        (
            vec!["x-special/gnome-copied-files", "image/png"],
            "cut",
            vec!["/files/a".to_owned()],
        ),
    ] {
        let (stream, _peer) = UnixStream::pair().unwrap();
        let mut conn = Conn::over(stream.into());
        let types = types.into_iter().map(str::to_owned).collect::<Vec<_>>();
        let got = read_offer_with(&mut conn, 10, &types, |_, _, mime| {
            Ok(match mime {
                "text/uri-list" => b"file:///files/a\r\n".to_vec(),
                "x-special/gnome-copied-files" => b"cut\nfile:///files/a\n".to_vec(),
                _ => panic!("must not read {mime}"),
            })
        })
        .unwrap();
        assert_eq!(got.op, expected);
        assert_eq!(got.paths, paths);
    }
}
