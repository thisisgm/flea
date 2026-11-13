# Alongside settings-keys: the same page at the narrow card width, including fractional-scale runs.
CASES+=("settings-keys-narrow 650x700 settings:keys")

case_settings_keys_narrow() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    settings_section keys
)
