"""Private terminal profiles and supported native launch arguments."""
import os
from pathlib import Path
import re
import shutil
import uuid

from .evidence import Blocked, digest, save
from .cells import PALETTE
from .geometry import COLUMNS, ROWS, ORIGIN

TERMINALS = ("alacritty", "foot", "ghostty", "kitty", "tmux")
# Every owned window's class and title start here; the class is also a valid GTK application ID.
OWNED_CLASS_PREFIX = "flea.battery."
# GLib's g_application_id_is_valid: dot-separated elements of [A-Za-z0-9_-], none empty or starting with a digit, 255 bytes at most.
APPLICATION_ID = re.compile(r"(?=.{1,255}$)[A-Za-z_-][A-Za-z0-9_-]*(\.[A-Za-z_-][A-Za-z0-9_-]*)+")
OPTIONS = {
    "alacritty": ("--config-file", "--class", "--title"),
    "foot": ("--config", "--app-id", "--title"),
    "ghostty": ("--config-file", "--gtk-single-instance", "--class"),
    "kitty": ("--config", "--class", "--title"),
    "tmux": (),
}
# kitty logs an unknown key or a bad line and still exits 0, so its check also fails on any output.
KITTY_CHECK = ("import sys\nfrom kitty.config import load_config\nbad = []\nload_config(sys.argv[-1], accumulate_bad_lines=bad)\n"
               "for line in bad:\n    print(f'{line.file}:{line.number}: {line.exception}: {line.line}', file=sys.stderr)\n"
               "sys.exit(1 if bad else 0)\n")
# Each terminal's own config checker as (arguments, any output fails); alacritty 0.17 ships none.
CHECKERS = {
    "foot": (lambda config: ["--check-config", "--config", config], False),
    "ghostty": (lambda config: ["+validate-config", f"--config-file={config}"], False),
    "kitty": (lambda config: ["+runpy", KITTY_CHECK, config], True),
}


def owned_identity():
    return OWNED_CLASS_PREFIX + "t" + uuid.uuid4().hex


def font_profile(profile):
    if profile is None:
        profile = {}
    if not isinstance(profile, dict):
        raise Blocked("invalid private font profile: expected an object")
    font = profile.get("font", "JetBrainsMono Nerd Font")
    size = profile.get("font_size", 14)
    if not isinstance(font, str) or not font.strip() or any(c in font for c in '\n\r"\\'):
        raise Blocked("invalid private font profile: font must be a nonempty single-line name")
    if type(size) not in (int, float) or not 4 <= size <= 48:
        raise Blocked("invalid private font profile: font_size must be a number from 4 to 48")
    return {"font": font, "font_size": size}


class Adapter:
    def __init__(self, name, directory, identity, palette, profile=None):
        if name not in TERMINALS:
            raise Blocked(f"unknown terminal: {name}")
        self.name, self.directory, self.identity = name, directory, identity
        self.palette, self.profile = palette, profile
        self.config = directory / ("terminal.toml" if name == "alacritty" else "terminal.conf")
        self.tmux_socket = None

    def preflight(self, commands, path=None, environment=None):
        executables = ["foot", "tmux"] if self.name == "tmux" else [self.name]
        receipts = []
        for executable in executables:
            resolved = shutil.which(executable, path=path)
            if not resolved:
                raise Blocked(f"missing terminal executable: {executable}")
            args = [resolved, "-V"] if executable == "tmux" else [resolved, "--help"]
            help_result = commands.run(args, env=environment)
            help_text = (help_result.stdout + help_result.stderr).decode(errors="replace")
            if executable == "ghostty" and any(option not in help_text for option in OPTIONS[executable]):
                config_help = commands.run([resolved, "+show-config", "--default", "--docs"], env=environment)
                help_text += (config_help.stdout + config_help.stderr).decode(errors="replace")
            for option in OPTIONS[executable]:
                if option not in help_text and not (executable == "ghostty" and option.removeprefix("--") in help_text):
                    raise Blocked(f"{executable} does not advertise required isolation option {option}")
            version_args = [resolved, "+version"] if executable == "ghostty" else [resolved, "--version"]
            if executable == "tmux":
                version_args = [resolved, "-V"]
            version = commands.run(version_args, env=environment)
            receipts.append({"executable": resolved, "sha256": digest(Path(resolved).resolve()),
                             "version": (version.stdout + version.stderr).decode(errors="replace").strip()})
        return receipts

    def configure(self):
        profile = font_profile(self.profile)
        font, size = profile["font"], profile["font_size"]
        palette = self.palette
        bg, fg = palette["bg"], palette["fg"]
        padding_x, padding_y = ORIGIN
        if self.name == "alacritty":
            content = (f'[window]\ndimensions = {{ columns = {COLUMNS}, lines = {ROWS} }}\n'
                       f'padding = {{ x = {padding_x}, y = {padding_y} }}\n'
                       f'decorations = "None"\n[font]\nnormal = {{ family = "{font}" }}\nsize = {size}\n'
                       f'[colors.primary]\nbackground = "{bg}"\nforeground = "{fg}"\n')
        elif self.name in ("foot", "tmux"):
            content = (f'[main]\nfont={font}:size={size}\ninitial-window-size-chars={COLUMNS}x{ROWS}\n'
                       f'pad={padding_x}x{padding_y}\n'
                       f'[colors-dark]\nbackground={bg[1:]}\nforeground={fg[1:]}\n')
        elif self.name == "ghostty":
            content = (f'font-family = {font}\nfont-size = {size}\nbackground = {bg}\nforeground = {fg}\n'
                       f'window-width = {COLUMNS}\nwindow-height = {ROWS}\n'
                       f'window-padding-x = {padding_x}\nwindow-padding-y = {padding_y}\n'
                       'window-decoration = false\nconfirm-close-surface = false\n')
        else:
            content = (f'font_family {font}\nfont_size {size}\nbackground {bg}\nforeground {fg}\n'
                       f'remember_window_size no\ninitial_window_width {COLUMNS}c\ninitial_window_height {ROWS}c\n'
                       'window_padding_width 0\nhide_window_decorations yes\nconfirm_os_window_close 0\n')
        if self.name == "alacritty":
            labels = ("black", "red", "green", "yellow", "blue", "magenta", "cyan", "white")
            for group, start in (("normal", 0), ("bright", 8)):
                content += f"[colors.{group}]\n" + "".join(f'{key} = "{PALETTE[start + i]}"\n' for i, key in enumerate(labels))
        elif self.name in ("foot", "tmux"):
            content += "".join(f'{"regular" if i < 8 else "bright"}{i % 8}={value[1:]}\n' for i, value in enumerate(PALETTE))
        elif self.name == "ghostty":
            content += "".join(f"palette = {i}={value}\n" for i, value in enumerate(PALETTE))
        else:
            content += "".join(f"color{i} {value}\n" for i, value in enumerate(PALETTE))
        self.config.write_text(content)
        if self.name == "tmux":
            (self.directory / "tmux.conf").write_text(
                'set -g status off\nset -g default-terminal tmux-256color\n'
                'set -g terminal-features "foot:RGB"\nset -g allow-passthrough off\n'
                'set -g extended-keys off\nset -g exit-empty on\nset -g destroy-unattached on\n')
        receipt = {"path": str(self.config), "sha256": digest(self.config), "font": font,
                   "font_size": size, "calibrated": False}
        if self.name == "tmux":
            receipt["tmux_config_sha256"] = digest(self.directory / "tmux.conf")
        return receipt

    def validate(self, commands, path=None, environment=None):
        """Runs the terminal's own checker on the config just written; a rejected config blocks the state, never a run on defaults."""
        executable = "foot" if self.name == "tmux" else self.name
        if executable not in CHECKERS:
            return {"status": "UNCHECKED", "reason": f"{executable} ships no config checker"}
        arguments, quiet = CHECKERS[executable]
        resolved = shutil.which(executable, path=path)
        if not resolved:
            raise Blocked(f"missing terminal executable: {executable}")
        result = commands.run([resolved, *arguments(str(self.config))], check=False,
                              env=self.environment() if environment is None else environment)
        lines = [line.strip() for line in (result.stderr + result.stdout).decode(errors="replace").splitlines() if line.strip()]
        if result.returncode or (quiet and lines):
            # foot prints its warnings before its error, so its first err: line names the rejection.
            line = next((line for line in lines if line.startswith("err:")), lines[0] if lines else f"exit {result.returncode}")
            raise Blocked(f"{executable} rejects the private terminal config: {line}")
        return {"status": "PASS", "checker": executable, "exit_code": result.returncode}

    def argv(self, command):
        app, config_path = self.identity, str(self.config)
        if self.name == "alacritty":
            return ["alacritty", "--config-file", config_path, "--class", app, "--title", app, "-e", *command]
        if self.name == "foot":
            return ["foot", "--config", config_path, "--app-id", app, "--title", app, *command]
        if self.name == "ghostty":
            # ghostty drops a --class GTK would refuse and maps as com.mitchellh.ghostty, which no owned-client query matches.
            if not APPLICATION_ID.fullmatch(app):
                raise Blocked(f"ghostty class must be a valid GTK application ID: {app}")
            return ["ghostty", "--config-default-files=false", f"--config-file={config_path}", f"--class={app}",
                    f"--title={app}", "--gtk-single-instance=false", "-e", *command]
        if self.name == "kitty":
            return ["kitty", "--config", config_path, "--class", app, "--name", app, "--title", app, *command]
        self.tmux_socket = str(self.directory / "tmux.sock")
        return ["foot", "--config", config_path, "--app-id", app, "--title", app,
                "tmux", "-S", self.tmux_socket, "-f", str(self.directory / "tmux.conf"),
                "new-session", "-s", "battery", *command]

    def environment(self):
        env = {key: os.environ[key] for key in ("PATH", "WAYLAND_DISPLAY", "DISPLAY", "XDG_RUNTIME_DIR",
               "HYPRLAND_INSTANCE_SIGNATURE", "DBUS_SESSION_BUS_ADDRESS", "LANG") if key in os.environ}
        home = self.directory / "terminal-home"
        home.mkdir(exist_ok=True)
        for key, part in (("HOME", ""), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache"), ("XDG_DATA_HOME", "data")):
            destination = home / part
            destination.mkdir(exist_ok=True)
            env[key] = str(destination)
        return env

    def close(self, commands):
        if self.tmux_socket and Path(self.tmux_socket).exists():
            result = commands.run(["tmux", "-S", self.tmux_socket, "kill-server"], check=False)
            message = result.stderr.decode(errors="replace")
            # exit-empty ends the private server with its last session, so after a quit there is nothing left to kill.
            if result.returncode and "no server running" not in message and "error connecting" not in message:
                raise Blocked(f"command exited {result.returncode}: tmux: {message[:300]}")
