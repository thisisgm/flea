//! The Combine card borrows the captured selection's held pictures, including late viewport answers.
use crate::{
    g2::print_convert::{model::Card, seam::Thumbnail},
    state::Shell,
};
use gpui::Context;
use std::{path::Path, sync::Arc};

impl Shell {
    pub(crate) fn sync_output_thumbnails(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.output.view.clone() else {
            return;
        };
        let current = view.read(cx);
        if !matches!(current.model.card, Some(Card::Combine(_))) {
            return;
        }
        let Some(selection) = current.model.selection.as_ref() else {
            return;
        };
        let generation = selection.generation;
        let Some(host) = self.panes[self.output.owner].as_ref() else {
            return;
        };
        let list = host.list.borrow();
        let pictures = selection
            .files
            .iter()
            .map(|source| {
                let index = host.pane.held.rows.iter().position(|row| {
                    Path::new(&host.path).join(&row.name) == Path::new(&source.path)
                })?;
                let row = &host.pane.held.rows[index];
                let picture = list.pictures.lend(host.pane.held.index_at(index), row)?;
                Some(Thumbnail {
                    path: source.path.clone(),
                    size: picture.decoded(),
                    image: picture.image.clone(),
                })
            })
            .collect::<Option<Vec<_>>>();
        let Some(pictures) = pictures else { return };
        let same = pictures.len() == current.thumbnails.len()
            && pictures.iter().zip(&current.thumbnails).all(|(a, b)| {
                a.path == b.path && a.size == b.size && Arc::ptr_eq(&a.image, &b.image)
            });
        drop(list);
        if !same {
            view.update(cx, |view, cx| {
                view.set_thumbnails(generation, pictures, cx);
            });
        }
    }
}
