// `flea plugin <verb>`, Omarchy's plugin verbs over the backend's own plugins request.
use crate::backend::plugins::{handle, Env};
use flea::i18n::I18n;
use flea::ui_logic_st2::wire::{self, Reply};
use flea::ui_logic_st2::sentence_key;

const VERBS: [&str; 7] = ["list", "add", "enable", "disable", "update", "remove", "validate"];

// Only a known verb takes the word, so a folder named plugin still opens.
pub fn is_command(args: &[String]) -> bool {
    args.get(1).map(String::as_str) == Some("plugin") && args.get(2).is_some_and(|v| VERBS.contains(&v.as_str()))
}

fn request(verb: &str, arg: &str) -> String {
    let (plugin, extra) = match verb {
        "add" | "validate" => ("", vec![("source", flea::jsondoc::Json::Str(arg.into()))]),
        _ => (arg, Vec::new()),
    };
    wire::request_line(0, verb, plugin, extra)
}

pub fn command(args: &[String]) -> i32 {
    let verb = args[2].as_str();
    let rest = &args[3..];
    let json = verb == "list" && rest == ["--json"];
    let arg = match (verb, rest) {
        ("list", []) => "",
        ("list", [_]) if json => "",
        ("list", _) => crate::usage("plugin list takes nothing, or --json"),
        (_, [one]) => one.as_str(),
        _ => crate::usage("plugin add, enable, disable, update, remove and validate take one argument"),
    };
    let env = match Env::user() {
        Ok(env) => env,
        Err(e) => {
            eprintln!("flea: {e}");
            return 2;
        }
    };
    let i18n = I18n::default();
    let mut code = 0;
    for line in handle(&request(verb, arg), &env) {
        match wire::parse(&line) {
            Ok(Reply::Outcome(o)) => {
                if let Some(e) = o.error {
                    eprintln!("flea: {}", i18n.tr_args(&sentence_key(e.code), &[&e.detail]));
                    code = 1;
                }
            }
            Ok(Reply::Listing(_)) if json => println!("{line}"),
            Ok(Reply::Listing(listing)) if verb == "list" => {
                for p in listing.plugins {
                    let state = match (&p.failure, p.enabled) {
                        (Some(_), _) => "failed",
                        (None, true) => "enabled",
                        (None, false) => "disabled",
                    };
                    println!("{}\t{}\t{}\t{}", p.manifest.id, state, p.manifest.version, p.manifest.kinds.join(","));
                }
            }
            _ => {}
        }
    }
    crate::backend::pluginruntime::shutdown();
    code
}
