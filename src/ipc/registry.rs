use super::wire::Reply;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Function {
    pub target: &'static str,
    pub name: &'static str,
    pub parameters: &'static [(&'static str, &'static str)],
    pub result: &'static str,
}

macro_rules! function {
    ($target:literal, $name:literal, $result:literal, $parameters:expr) => {
        Function {
            target: $target,
            name: $name,
            result: $result,
            parameters: $parameters,
        }
    };
}

pub const ORACLE: &[Function] = &include!("functions.rs");
pub const LIFECYCLE: &[Function] = &[
    function!("flea", "version", "string", &[]),
    function!("flea", "quit", "void", &[]),
    function!("fleapicker", "version", "string", &[]),
    function!("fleapicker", "quit", "void", &[]),
];

pub const VISUAL: &[Function] = &[
    function!(
        "flea",
        "scene",
        "string",
        &[
            ("window", "string"),
            ("name", "string"),
            ("settings", "string")
        ]
    ),
    function!(
        "flea",
        "nativeGeometry",
        "string",
        &[
            ("window", "string"),
            ("scene", "string"),
            ("frame", "string")
        ]
    ),
    function!("flea", "pendingRequests", "int", &[]),
    function!("flea", "previewReadiness", "string", &[]),
    function!("flea", "sceneState", "string", &[]),
    function!("flea", "presentedFrame", "string", &[]),
];

pub fn functions() -> impl Iterator<Item = &'static Function> {
    ORACLE.iter().chain(LIFECYCLE).chain(VISUAL)
}

pub fn validate(
    target: &str,
    name: &str,
    arguments: &[String],
) -> Result<&'static Function, Reply> {
    if !["flea", "fleapicker"].contains(&target) {
        return Err(Reply::value("Target not found."));
    }
    let function = functions()
        .find(|f| f.target == target && f.name == name)
        .ok_or_else(|| Reply::value("Function not found."))?;
    if function.parameters.len() != arguments.len() {
        let amount = if arguments.len() < function.parameters.len() {
            "few"
        } else {
            "many"
        };
        return Err(Reply::value(format!("Too {amount} arguments provided ({} required but {} were provided.)\nFunction definition: {}",
            function.parameters.len(), arguments.len(), signature(function))));
    }
    for (index, ((_, kind), value)) in function.parameters.iter().zip(arguments).enumerate() {
        let valid = match *kind {
            "int" => value.trim().parse::<i32>().is_ok(),
            "bool" => ["true", "false", "1", "0"].contains(&value.as_str()),
            "string" => true,
            _ => false,
        };
        if !valid {
            return Err(Reply::value(format!("Unable to parse argument {} as \"{kind}\". Provided argument: \"{value}\"\nFunction definition: {}",
                index + 1, signature(function))));
        }
    }
    Ok(function)
}

fn signature(function: &Function) -> String {
    let parameters = function
        .parameters
        .iter()
        .map(|(name, kind)| format!("{name}: {kind}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "function {}({parameters}): {}",
        function.name, function.result
    )
}

pub fn refused(function: &Function) -> Reply {
    Reply::error(
        3,
        format!("FLEA_IPC_NOT_PORTED {}.{}", function.target, function.name),
    )
}

pub fn show() -> Reply {
    let mut output = String::new();
    for target in ["flea", "fleapicker"] {
        output.push_str(&format!("target {target}\n"));
        for function in functions().filter(|f| f.target == target) {
            output.push_str(&format!("  {}\n", signature(function)));
        }
    }
    Reply {
        code: 0,
        stdout: output,
        stderr: String::new(),
    }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
