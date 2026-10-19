//! The IPC reader for the collision card; every other dialog's reader belongs to its own unit.
use super::model::Operations;
use crate::ipc::Adapter;
use flea::ipc::wire::Reply;

pub struct OperationsAdapter<'a> {
    pub model: &'a Operations,
}

impl Adapter for OperationsAdapter<'_> {
    fn call(&self, function: &str, _arguments: &[String]) -> Option<Reply> {
        match function {
            "collideState" => Some(Reply::value(self.model.collide_state())),
            _ => None,
        }
    }
}
