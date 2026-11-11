//! Plugins: the manifest, its settings schema, where its state lives, the backend wire and the Settings rows drawn for it.
pub mod airdrop;
pub mod builtin;
pub mod contributions;
pub mod manifest;
pub mod pages;
pub mod schema;
pub mod shelf;
pub mod store;
pub mod wire;

// Refusals of a request rather than of a manifest; each selects plugins.error.<code>.
pub const ERROR_CODES: [&str; 10] = [
    "not_found", "no_manifest", "exists", "git", "write", "unknown", "builtin", "setting", "busy", "request",
];

// The catalog key a refusal's code selects; the detail is its %1.
pub fn sentence_key(code: &str) -> String {
    if ERROR_CODES.contains(&code) {
        format!("plugins.error.{code}")
    } else {
        format!("plugins.invalid.{code}")
    }
}

#[cfg(test)]
mod shelf_tests;
#[cfg(test)]
mod tests;
