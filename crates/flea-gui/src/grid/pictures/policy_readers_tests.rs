//! A cache observer keeps completed sources while the drawing policy disables thumbnails.
use super::{Pictures, Policy, Slot};
use crate::{
    grid::element::{Identity, PictureKind, Thumbnail},
    list::pane::Held,
};
use flea::{jsondoc, protocol::RowFacts};
use gpui::RenderImage;
use std::sync::Arc;

#[test]
fn fx_uish_thumbnail_observer_retains_ready_sources_without_enabling_draws() {
    let pictures = Pictures::default();
    let mut held = Held {
        listing: 7,
        total: 1,
        start: 0,
        rows: vec![RowFacts {
            name: "a.png".into(),
            size: 1,
            mtime: 1,
            ..Default::default()
        }],
    };
    let policy = Policy::read(&jsondoc::parse(r#"{"preview":{"thumbnails":"off"}}"#).unwrap());
    {
        let mut map = pictures.map.borrow_mut();
        map.listing = held.listing;
        map.path = "/files".into();
        map.policy = Some(policy);
        map.remember(
            0,
            Slot::Ready {
                file: "/cache/a.png".into(),
                image: true,
                picture: Arc::new(Thumbnail {
                    identity: Identity {
                        name: "a.png".into(),
                        size: 1,
                        mtime: 1,
                    },
                    kind: PictureKind::Image,
                    image: Arc::new(RenderImage::new(vec![image::Frame::new(
                        image::RgbaImage::new(1, 1),
                    )])),
                }),
            },
        );
    }
    assert_eq!(
        pictures.policy_files(&held, "/files", policy).get(&0),
        Some(&Some("/cache/a.png".into()))
    );
    assert!(
        !pictures.ready(&held, 0),
        "Off must keep the renderer disabled"
    );
    assert!(pictures.policy_files(&held, "/other", policy).is_empty());
    held.listing += 1;
    assert!(pictures.policy_files(&held, "/files", policy).is_empty());
    held.listing -= 1;
    held.rows[0].mtime += 1;
    assert!(pictures.policy_files(&held, "/files", policy).is_empty());
}
