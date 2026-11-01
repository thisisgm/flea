//! Resolve bindings against translated samples from the actual active keyboard group.
use super::{builtin, UserBinding, UserKeys, Verdict, MAX_BINDINGS, RESERVED};
use crate::keys::{lookup_either, Context, Frontend, Input, Match};

#[derive(Clone, Debug, PartialEq, Eq)]
struct OwnedInput {
    key: String,
    text: String,
    modifiers: u8,
}

impl OwnedInput {
    fn new(input: Input<'_>) -> Self {
        Self {
            key: input.key.into(),
            text: input.text.into(),
            modifiers: input.modifiers,
        }
    }

    fn input(&self) -> Input<'_> {
        Input {
            key: &self.key,
            text: &self.text,
            modifiers: self.modifiers,
        }
    }
}

/// One raw stroke's two inputs, kept together in the same order as native lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sample {
    first: OwnedInput,
    second: Option<OwnedInput>,
}

impl Sample {
    pub fn new(inputs: [Option<Input<'_>>; 2]) -> Option<Self> {
        Some(Self {
            first: OwnedInput::new(inputs[0]?),
            second: inputs[1].map(OwnedInput::new),
        })
    }

    fn matches(&self, binding: &UserBinding) -> bool {
        binding.matches(self.first.input())
            || self
                .second
                .as_ref()
                .is_some_and(|input| binding.matches(input.input()))
    }

    fn action(&self, preset: &str) -> Match {
        lookup_either(
            preset,
            Context::Listing,
            Frontend::Gui,
            self.first.input(),
            self.second.as_ref().map(OwnedInput::input),
        )
    }
}

/// Supported modifier masks each include both Caps states; absent masks stay unsupported.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Projection {
    pub modifiers: Vec<u8>,
    pub samples: Vec<Sample>,
}

impl UserKeys {
    /// Analyze potential conflicts, without claiming a raw sample survives native composition.
    pub fn verdicts(
        &self,
        preset: &str,
        plugin: &dyn Fn(&str) -> bool,
        projection: &Projection,
    ) -> Vec<Verdict> {
        let preset = crate::keys::preset(preset);
        let actions: Vec<_> = projection
            .samples
            .iter()
            .map(|sample| sample.action(preset))
            .collect();
        let mut claimed = vec![false; projection.samples.len()];
        self.bindings
            .iter()
            .map(|binding| {
                let verdict = if !binding.applies(preset)
                    || binding.is_plugin() && !plugin(&binding.action)
                {
                    Verdict::Inactive
                } else if !binding.is_plugin() && builtin(&binding.action).is_none() {
                    Verdict::Unknown
                } else if binding.mods != "text"
                    && RESERVED.iter().any(|(mask, key)| {
                        *mask == binding.mask && binding.key.eq_ignore_ascii_case(key)
                    })
                {
                    Verdict::Taken("")
                } else if binding
                    .required_modifiers()
                    .iter()
                    .any(|mask| !projection.modifiers.contains(mask))
                {
                    Verdict::Unavailable
                } else {
                    let mut reachable = false;
                    let mut taken = None;
                    let mut repeated = false;
                    for ((sample, action), claimed) in
                        projection.samples.iter().zip(&actions).zip(&claimed)
                    {
                        if !sample.matches(binding) {
                            continue;
                        }
                        reachable = true;
                        repeated |= *claimed;
                        match action {
                            Match::Action(action) => {
                                taken.get_or_insert(*action);
                            }
                            Match::Suppressed => {
                                taken.get_or_insert("");
                            }
                            Match::Unbound => {}
                        }
                    }
                    if let Some(action) = taken {
                        Verdict::Taken(action)
                    } else if repeated {
                        Verdict::Repeated
                    } else if reachable {
                        Verdict::Live
                    } else {
                        Verdict::Unavailable
                    }
                };
                if verdict == Verdict::Live {
                    for (sample, claimed) in projection.samples.iter().zip(&mut claimed) {
                        *claimed |= sample.matches(binding);
                    }
                }
                verdict
            })
            .collect()
    }

    /// Dispatch uses the same resolved standing as Settings, without allocating or resampling.
    pub fn find(
        &self,
        preset: &str,
        input: Input<'_>,
        plugin: &dyn Fn(&str) -> bool,
        verdicts: &[Verdict],
    ) -> Option<&UserBinding> {
        if verdicts.len() != self.bindings.len() {
            return None;
        }
        let preset = crate::keys::preset(preset);
        self.bindings
            .iter()
            .zip(verdicts)
            .take(MAX_BINDINGS)
            .find_map(|(binding, verdict)| {
                (*verdict == Verdict::Live
                    && binding.applies(preset)
                    && binding.matches(input)
                    && if binding.is_plugin() {
                        plugin(&binding.action)
                    } else {
                        builtin(&binding.action).is_some()
                    })
                .then_some(binding)
            })
    }
}
