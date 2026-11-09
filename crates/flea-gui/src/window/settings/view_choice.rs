//! The view a Settings choice or a view key picks, and how ui.json remembers it.
use crate::{state::Shell, visual::view_name, window::startup::View};
use flea::{jsondoc::Json, ui_logic_settings_pages::pages};
use gpui::{Context, Window};

impl Shell {
    pub(crate) fn choose_settings_view(
        &mut self,
        view: View,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.lifecycle.phase() != crate::window::lifecycle::Phase::Open {
            return;
        }
        let from_dual = self.view == View::Dual;
        self.set_view(view, window, cx);
        self.remember_settings_view(view, from_dual, window, cx);
    }

    // Pane.qml chooseView: Dual is a window layout and no folder's view, so choosing it, or any view from it, sets the standing view.
    fn remember_settings_view(
        &mut self,
        view: View,
        from_dual: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = Json::Str(view_name(view).into());
        let per_folder = view != View::Dual
            && pages::is_true(&self.launch.settings.data, &["rememberViewPerFolder"]);
        let mut patch = Vec::new();
        if !per_folder || from_dual {
            patch.push(("view".into(), name.clone()));
        }
        if per_folder {
            let Some(host) = &self.panes[self.focus_side] else {
                return;
            };
            patch.push((
                "folderViews".into(),
                Json::Obj(vec![(host.path.clone(), name)]),
            ));
        }
        self.change_settings(Json::Obj(patch), window, cx);
    }
}
