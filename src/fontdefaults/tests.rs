use super::*;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("flea-font-native-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir(&path).unwrap(); Self(path)
    }
    fn put(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name); fs::create_dir_all(path.parent().unwrap()).unwrap(); fs::write(&path, text).unwrap(); path
    }
}
impl Drop for Scratch { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
const FAMILY: &str = "Flea \"Demo\" & <Sans> \\ Regular";
#[test]
fn inline_styles_preserve_size_offsets_and_comments() {
    let text = "# keep\n[window]\nopacity=0.8\n[font]\nsize=17\nnormal={family='Old',style='Regular'} # keep style\n[font.offset]\nx=2\ny=3\n";
    let out = alacritty::edit(text, FAMILY).unwrap();
    assert!(out.starts_with("# keep\n[window]\nopacity=0.8\n[font]\nsize=17\n"));
    assert!(out.contains("style='Regular'} # keep style"));
    assert!(out.ends_with("[font.offset]\nx=2\ny=3\n"));
    assert_eq!(out.matches("Flea").count(), 4);
}
#[test]
fn expanded_tables_and_dotted_assignments() {
    for text in ["[font.normal]\nfamily='Old'\nstyle='Light'\n[window]\nopacity=0.8\n", "font.normal.family = 'Old'\nfont.size = 15\n"] {
        let out = alacritty::edit(text, "New").unwrap();
        assert!(!out.contains("Old")); assert_eq!(out.matches("New").count(), 4);
    }
}
#[test]
fn root_inline_font_adds_missing_styles_inside_the_table() {
    let text = "font = { size = 16, normal = { style = 'Light' } }\n[window]\nopacity=0.8\n";
    let out = alacritty::edit(text, "New").unwrap();
    assert!(!out.contains("[font.")); assert_eq!(out.matches("New").count(), 4); assert!(out.contains("size = 16"));
}
#[test]
fn multiline_strings_cannot_create_fake_font_tables() {
    let text = "title = '''\n[font]\nnormal={family='Decoy'}\n'''\n[font]\nsize=18\n";
    let out = alacritty::edit(text, "New").unwrap();
    assert!(out.contains("normal={family='Decoy'}")); assert_eq!(out.matches("New").count(), 4);
}
#[test]
fn unsupported_and_ambiguous_font_layouts_are_refused() {
    for text in ["font=3", "font.normal=false", "[font]\nnormal={family='A',family='B'}", "[font]\n[font]\n", "font={}\n[font.normal]\n", "[[font]]\n", "[font]\nnormal={family='unterminated}\n"] {
        assert!(alacritty::edit(text, "New").is_err(), "{text}");
    }
}
#[test]
fn quoted_table_and_family_keys_are_supported() {
    let out = alacritty::edit("['font'.\"normal\"]\n'family'='Old'\n", "New").unwrap();
    assert!(!out.contains("Old")); assert_eq!(out.matches("New").count(), 4);
}
#[test]
fn repeated_apply_does_not_accumulate_kitty_overrides() {
    let scratch = Scratch::new();
    let path = scratch.put("kitty/kitty.conf", "font_size 17\nfont_family Old\n");
    let first = edit::changes(&scratch.0, "kitty", FAMILY).unwrap().remove(0).1;
    fs::write(&path, &first).unwrap();
    let second = edit::changes(&scratch.0, "kitty", FAMILY).unwrap().remove(0).1;
    assert_eq!(first, second); assert_eq!(second.matches("font_family").count(), 1); assert!(second.contains("font_size 17"));
}
#[test]
fn gtk_and_foot_keep_sizes_and_unrelated_sections() {
    let scratch = Scratch::new();
    scratch.put("gtk-3.0/settings.ini", "[Settings]\ngtk-font-name=Old 17\ngtk-theme-name=Theme\n");
    let changes = edit::changes(&scratch.0, "desktop", FAMILY).unwrap();
    assert!(changes[1].1.contains("Regular 17\ngtk-theme-name=Theme"));
    scratch.put("foot/foot.ini", "[main]\nfont=Old:size=17,Fallback\nfont-bold=Old Bold:size=19\n[scrollback]\nlines=2000\n");
    let out = edit::changes(&scratch.0, "foot", FAMILY).unwrap().remove(0).1;
    assert!(out.contains(":size=17,Fallback")); assert!(out.contains(":size=19")); assert!(out.ends_with("[scrollback]\nlines=2000\n"));
}
#[test]
fn ghostty_retains_fallbacks_and_prefers_the_modern_config_name() {
    let scratch = Scratch::new();
    scratch.put("ghostty/config", "font-family = Legacy\n");
    let modern = scratch.put("ghostty/config.ghostty", "font-family=Old\nfont-family=Fallback\nfont-size=17\n");
    let (path, out) = edit::changes(&scratch.0, "ghostty", FAMILY).unwrap().remove(0);
    assert_eq!(path, modern); assert!(out.contains("font-family=Fallback")); assert!(out.contains("font-size=17"));
}
#[test]
fn gvariant_and_legacy_json_string_escapes_round_trip() {
    let family = "O'Font \\ \"quoted\" á";
    assert_eq!(settings::decode(&settings::quote(family)).unwrap(), family);
    assert_eq!(settings::decode(&format!("\"{}\"", crate::json::escape(family))).unwrap(), family);
    assert_eq!(settings::size("Old 17.5"), "17.5"); assert_eq!(settings::size("Old Regular"), "11");
}
#[test]
fn native_apply_restores_original_modes_links_and_bytes() {
    let scratch = Scratch::new();
    let original = "font_size 17\r\nfont_family Old\r\n";
    let real = scratch.put("dotfiles/kitty", original);
    fs::set_permissions(&real, fs::Permissions::from_mode(0o640)).unwrap();
    let config = scratch.0.join("config"); fs::create_dir_all(config.join("kitty")).unwrap();
    let link = config.join("kitty/kitty.conf"); symlink(&real, &link).unwrap();
    let state = scratch.0.join("state"); let _lock = files::lock(&state).unwrap();
    mutate(&config, &state, "apply", "kitty", "First").unwrap();
    mutate(&config, &state, "apply", "kitty", "Second").unwrap();
    mutate(&config, &state, "restore", "kitty", "").unwrap();
    assert_eq!(fs::read_to_string(real).unwrap(), original); assert_eq!(files::mode(&link), 0o640); assert!(link.is_symlink());
}
#[test]
fn native_restore_preserves_external_edits() {
    let scratch = Scratch::new();
    let config = scratch.0.join("config");
    let path = scratch.put("config/kitty/kitty.conf", "font_family Old\n");
    let state = scratch.0.join("state"); let _lock = files::lock(&state).unwrap();
    mutate(&config, &state, "apply", "kitty", "New").unwrap();
    fs::write(&path, "# external edit\n").unwrap();
    assert!(mutate(&config, &state, "restore", "kitty", "").is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), "# external edit\n");
}
#[test]
fn python_version_journal_remains_restorable() {
    let scratch = Scratch::new();
    let config = scratch.0.join("config"); let state = scratch.0.join("state");
    let path = scratch.put("config/kitty/kitty.conf", "font_family Applied\n");
    let journal = format!(r#"{{"family":"Applied","files":[{{"path":"{}","original":"font_family Original\n","applied":"font_family Applied\n","mode":416}}],"setting":null}}"#, crate::json::escape(&path.to_string_lossy()));
    scratch.put("state/kitty.json", &journal);
    mutate(&config, &state, "restore", "kitty", "").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "font_family Original\n"); assert_eq!(files::mode(&path), 0o640);
    assert!(!state.join("kitty.json").exists());
}
#[test]
fn fresh_defaults_restore_removes_the_created_file() {
    let scratch = Scratch::new(); let _lock = files::lock(&scratch.0.join("state")).unwrap();
    mutate(&scratch.0, &scratch.0.join("state"), "apply", "serif", FAMILY).unwrap();
    mutate(&scratch.0, &scratch.0.join("state"), "restore", "serif", "").unwrap();
    assert!(!scratch.0.join("fontconfig/conf.d/99-flea-serif.conf").exists());
}
#[test]
fn installed_terminal_parsers_accept_escaped_native_preferences() {
    let scratch = Scratch::new();
    scratch.put("kitty/kitty.conf", "font_size 17\n");
    scratch.put("ghostty/config", "font-size=17\nfont-family=Old\nfont-family=Fallback\n");
    scratch.put("foot/foot.ini", "font=Old:size=17\n[scrollback]\nlines=2000\n");
    for target in ["kitty", "ghostty", "foot"] {
        if !files::executable(target) { continue; }
        let (path, text) = edit::changes(&scratch.0, target, FAMILY).unwrap().remove(0);
        fs::write(&path, text).unwrap();
        let mut command = Command::new("timeout"); command.args(["10", target]);
        match target {
            "kitty" => { command.args(["+runpy", "from kitty.config import load_config; import sys; bad=[]; load_config(sys.argv[1], accumulate_bad_lines=bad); assert not bad, bad"]).arg(&path); }
            "ghostty" => { command.arg("+show-config"); }
            _ => { command.arg("--check-config").arg(format!("--config={}", path.display())); }
        }
        let output = command.env("HOME", &scratch.0).env("XDG_CONFIG_HOME", &scratch.0).output().unwrap();
        assert!(output.status.success(), "{target}: {}", String::from_utf8_lossy(&output.stderr));
        if target == "ghostty" { assert!(!String::from_utf8_lossy(&output.stderr).to_lowercase().contains("error")); }
    }
}
