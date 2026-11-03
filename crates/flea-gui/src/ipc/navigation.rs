//! Read-only navigation adapters: the header's sort mark is the order the listing is actually in.
use super::Adapter;
use crate::navigation::Navigator;
use flea::ipc::wire::Reply;

pub struct NavigationAdapter<'a> {
    pub navigator: &'a Navigator,
}

impl Adapter for NavigationAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        match function {
            "sortMark" => Some(Reply::value(self.navigator.sort_mark())),
            _ => None,
        }
    }
}
