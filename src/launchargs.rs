use crate::tui::cwd;

#[derive(Default)]
pub struct Options {
    pub want_tui: bool,
    pub print_target: bool,
    pub tui_options: cwd::Options,
    pub select_raw: Option<String>,
    pub start: Option<String>,
}

impl Options {
    // Sample argv: ["flea", "--tui", "--cwd-file", "/tmp/cwd", "--select", "a.txt", "/work"].
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self::default();
        let mut want_gui = false;
        let mut i = 1;
        while i < args.len() {
            if options.tui_options.take(args, &mut i)? {
                i += 1;
                continue;
            }
            match args[i].as_str() {
                "--tui" => options.want_tui = true,
                "--gui" => want_gui = true,
                "--print-target" => options.print_target = true,
                "--select" => {
                    i += 1;
                    match args.get(i) {
                        Some(value) => options.select_raw = Some(value.clone()),
                        None => return Err("--select needs a target".into()),
                    }
                }
                a if a.starts_with("--") => return Err(format!("unknown flag {}", a)),
                a => options.start = options.start.or_else(|| Some(a.to_string())),
            }
            i += 1;
        }
        if options.want_tui && want_gui {
            return Err("--tui and --gui cannot be used together".into());
        }
        options.tui_options.validate(options.want_tui, options.print_target)?;
        Ok(options)
    }
}
