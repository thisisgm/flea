use super::{Plan, MAX};
use flea::jsondoc::Json;

// Sample input: {"startIn":"last","lastTabs":{"paths":["/home/a","/home/b"],"index":1}}.
// Without a Last-folder strip or with a named path there is no plan; a non-string entry voids the strip.
pub fn restore_plan(state: &Json, named: &str) -> Option<Plan> {
    if !named.is_empty() {
        return None;
    }
    if state
        .get("startIn")
        .and_then(Json::as_str)
        .unwrap_or("home")
        != "last"
    {
        return None;
    }
    let stored = state.get("lastTabs");
    let kept = stored
        .and_then(|value| value.get("paths"))
        .and_then(Json::as_array)
        .unwrap_or(&[]);
    let mut paths = Vec::new();
    for entry in kept {
        match entry.as_str() {
            Some(path) if !path.is_empty() => {
                if paths.len() < MAX {
                    paths.push(path.to_owned());
                }
            }
            _ => return None,
        }
    }
    if paths.is_empty() {
        return None;
    }
    let index = stored
        .and_then(|value| value.get("index"))
        .and_then(|value| match value {
            Json::Num(_) => value.as_f64(),
            _ => None,
        })
        .filter(|index| index.is_finite() && index.fract() == 0.)
        .unwrap_or(0.);
    let last = (paths.len() - 1) as f64;
    Some(Plan {
        index: index.clamp(0., last) as usize,
        paths,
    })
}
