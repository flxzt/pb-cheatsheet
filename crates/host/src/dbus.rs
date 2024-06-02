use anyhow::Context;
use pb_cheatsheet_com::FocusedWindowInfo;
use zbus::{proxy, Connection, Result};

#[proxy(
    default_service = "org.gnome.Shell",
    default_path = "/org/gnome/shell/extensions/FocusedWindow",
    interface = "org.gnome.shell.extensions.FocusedWindow"
)]
trait FocusedWindow {
    async fn get(&self) -> Result<String>;
}

pub(crate) async fn get_focused_window_info<'a>(
    connection: &Connection,
) -> anyhow::Result<FocusedWindowInfo> {
    let proxy = FocusedWindowProxy::new(connection).await?;
    let val: serde_json::Value = serde_json::from_str(&proxy.get().await?)?;
    let title = trim_parentheses(val["title"].to_string());
    let wm_class = trim_parentheses(val["wm_class"].to_string());
    let wm_class_instance = trim_parentheses(val["wm_class_instance"].to_string());
    let pid = val["pid"].as_u64().context("Converting 'pid' to 'u64'")?;
    let focus = val["focus"]
        .as_bool()
        .context("Converting 'focus' to 'bool'")?;

    Ok(FocusedWindowInfo {
        title,
        wm_class,
        wm_class_instance,
        pid,
        focus,
    })
}

fn trim_parentheses(content: String) -> String {
    const PATTERN: [char; 2] = ['"', '\''];
    content
        .trim()
        .trim_start_matches(PATTERN)
        .trim_end_matches(&PATTERN)
        .to_string()
}
