//! The picker sends the main browser's effective folder order with its initial screenful.
use super::listing::Listing;
use crate::navigation::folder_state::{stored_key, Folders, Order};
use flea::{
    json::escape,
    jsondoc::{self, Json},
    ui_logic_picker_listing::{self as core, FilterDef},
};

impl Listing {
    pub fn set_filter(&mut self, filter: Option<FilterDef>, chip: i64) {
        self.filter = filter;
        self.chip = chip;
    }

    pub fn set_hidden(&mut self, show: bool) {
        self.show_hidden = show;
    }

    // The list request; caller chip and hidden ride along.
    pub fn request_line(&self, first: usize) -> String {
        format!(
            r#"{{"c":"list","path":"{}","first":{},{}{}}}"#,
            escape(&self.location),
            first,
            core::list_fields(self.filter.as_ref(), self.show_hidden),
            self.sort_fields()
        )
    }

    pub(super) fn apply_settings(&mut self, data: &Json) -> bool {
        let before = self.sort_options();
        self.folders = Some(Folders::from_settings(data, Vec::new()));
        before != self.sort_options()
    }

    /// A title tap's order: the folder keeps it while remembering is on and the default follows every sort, as PickerWindow.qml requestSort writes them; answers that ui.json patch.
    pub fn choose_order(&mut self, order: Order) -> Json {
        let folders = self
            .folders
            .get_or_insert_with(|| Folders::from_settings(&Json::Null, Vec::new()));
        folders.global = order;
        let mut patch = vec![(
            "sort".to_owned(),
            Json::Obj(vec![
                ("key".into(), Json::Str(stored_key(order.key).into())),
                ("reverse".into(), Json::Bool(order.desc)),
            ]),
        )];
        if !core::is_recent(&self.location) {
            if let Some(Json::Obj(folder)) = folders
                .remember(&self.location, order)
                .and_then(|text| jsondoc::parse(&text).ok())
            {
                patch.extend(folder);
            }
        }
        Json::Obj(patch)
    }

    pub fn order(&self) -> Order {
        self.sort_options().0
    }

    fn sort_options(&self) -> (Order, bool, bool, bool) {
        self.folders
            .as_ref()
            .map_or((Order::default(), true, false, false), |folders| {
                (
                    folders.order_for(&self.location),
                    folders.folders_first,
                    folders.group_by_kind,
                    folders.hidden_last,
                )
            })
    }

    pub(super) fn sort_fields(&self) -> String {
        let (order, first, group, hidden) = self.sort_options();
        if (order, first, group, hidden) == (Order::default(), true, false, false) {
            return String::new();
        }
        format!(
            r#","by":"{}","desc":{},"foldersFirst":{},"groupByKind":{},"hiddenLast":{}"#,
            order.key, order.desc, first, group, hidden
        )
    }
}
