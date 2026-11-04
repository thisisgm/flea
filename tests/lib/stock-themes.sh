# Stock theme census shared by tests/js.sh and tests/themes.sh.

# A stock theme is a directory holding colors.toml, as tests/gpui-theme-gate.sh counts; a retired theme can leave its directory behind.
stock_themes_installed() {
    local palette name
    for palette in "$1"/*/colors.toml; do
        [ -f "$palette" ] || continue
        name=${palette%/colors.toml}
        printf '%s\n' "${name##*/}"
    done | sort
}

# Sample input, the block this reads out of tests/js/themes.js:
#   var THEMES = ["catppuccin", "catppuccin-latte", "ethereal",
#                 "everforest", ...]
stock_themes_pinned() {
    sed -n '/^var THEMES = \[/,/\]$/p' "$1" | grep -o '"[a-z0-9-]*"' | tr -d '"' | sort
}
