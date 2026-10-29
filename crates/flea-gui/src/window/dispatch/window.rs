//! The window family: Add a Network Share and the keys the network menu and share browser resolve.
use super::Order;
use crate::state::Shell;
use gpui::{Context, Window};

pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    match order {
        Order::Network(event) => shell.network_run(event.action, window, cx),
        Order::Action(event) if event.action == "addNetwork" => {
            shell.network_add(shell.focus_side, window, cx);
            true
        }
        Order::Action(event) if event.action == "extThumbs" => {
            toggle_ext_thumbs(shell, shell.focus_side, window, cx);
            true
        }
        Order::Action(event) if event.action == "updateFlea" => {
            shell.settings_update(true, window, cx);
            true
        }
        _ => false,
    }
}

pub(super) fn toggle_ext_thumbs(
    shell: &mut Shell,
    side: usize,
    window: &mut Window,
    cx: &mut Context<Shell>,
) {
    use flea::{jsondoc::Json, ui_logic_st1::edit};
    let class = shell.panes[side]
        .as_ref()
        .and_then(|host| host.sizes.class())
        .unwrap_or("")
        .to_owned();
    let (key, path) = match class.as_str() {
        "network" => ("thumbNetwork", "preview.thumbNetwork"),
        "phone" => ("thumbPhone", "preview.thumbPhone"),
        "usb" => ("thumbUsb", "preview.thumbUsb"),
        _ => return,
    };
    let setting = shell
        .launch
        .settings
        .data
        .get("preview")
        .and_then(|preview| preview.get(key))
        .and_then(Json::as_bool);
    let on = !(if class == "usb" {
        setting != Some(false)
    } else {
        setting == Some(true)
    });
    shell.change_settings(edit::setting(path, Json::Bool(on)), window, cx);
    if let Some(sentence) =
        flea::ui_logic_pv1::property::thumbs_status(&class, on, &shell.locale.catalog)
    {
        if let Some(host) = &mut shell.panes[side] {
            host.status = sentence.clone().into();
        }
        shell.status_messages(cx, |messages, now| {
            messages.say(
                &sentence,
                false,
                "",
                flea::ui_logic_fb1::status::Hint::None,
                now,
            );
        });
    }
}
