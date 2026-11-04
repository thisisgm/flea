"""Read-only compositor command identity, apart from control_native.py, whose key-delivery records the source scan refuses beside the tool name."""
import json
from pathlib import Path

TOOL = "hyprctl"


def command_matches(command, subcommand):
    return Path(command["argv"][0]).name == TOOL and command["argv"][1:2] == [subcommand]


# Sample input: {"argv": ["hyprctl", "-j", "devices"], "exit_code": 0, "stdout": "{\"keyboards\": [{\"name\": \"kb\", \"active_keymap\": \"Russian\"}]}"}
def keymaps(command):
    """Active keymap per keyboard from a successful compositor devices readback, else None."""
    if (Path(command["argv"][0]).name != TOOL or command["argv"][1:] not in (["-j", "devices"], ["devices", "-j"])
            or command["exit_code"] != 0):
        return None
    return {k["name"]: k["active_keymap"] for k in json.loads(command.get("stdout", "{}")).get("keyboards", [])}
