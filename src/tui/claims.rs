use super::model::Model;
use super::wire::Wire;
use crate::jsondoc::Json;
use std::io;

// Feature replies claimed before Model's own match; the first module that takes the reply ends the chain.
pub fn receive(m: &mut Model, value: &Json, w: &mut Wire) -> io::Result<bool> {
    Ok(super::parity17::receive(m, value, w)?
        || super::gone::receive(m, value, w)?
        || super::combine::receive(m, value, w)?
        || super::convert::receive(m, value, w)?
        || super::print::receive(m, value, w)?
        || super::contents::receive(m, value, w)?)
}
