use inkview::bindings::Inkview;
use std::ffi::{c_char, CStr};

pub fn wifi_activate(iv: &Inkview, show_hourglass: bool) -> anyhow::Result<()> {
    if wifi_check_connected(iv)? {
        return Ok(());
    }
    try_connect(iv, show_hourglass)?;
    if wifi_check_connected(iv)? {
        return Ok(());
    }
    Ok(())
}

pub fn wifi_keepalive(iv: &Inkview) -> anyhow::Result<()> {
    wifi_activate(iv, false)
}

pub fn wifi_check_connected(iv: &Inkview) -> anyhow::Result<bool> {
    let netinfo = unsafe {
        iv.NetInfo()
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("inkview 'NetInfo()' returned NULL"))?
    };
    let is_connected = netinfo.connected != 0;
    Ok(is_connected)
}

fn try_connect(iv: &Inkview, show_hourglass: bool) -> anyhow::Result<Option<String>> {
    let network_name = std::ptr::null() as *const c_char;

    #[cfg(any(feature = "sdk-5-19", feature = "sdk-6-5"))]
    let res = {
        let show_hourglass = if show_hourglass { 1 } else { 0 };
        unsafe { iv.NetConnect2(network_name, show_hourglass) }
    };
    #[cfg(feature = "sdk-6-8")]
    let res = { unsafe { iv.NetConnect2(network_name, show_hourglass) } };

    if res != 0 {
        return Err(anyhow::anyhow!(
            "inkview 'NetConnect2()' returned with non-zero code: '{res}'"
        ));
    }
    if network_name.is_null() {
        return Ok(None);
    }
    let network_name = unsafe { CStr::from_ptr(network_name).to_owned() };
    Ok(Some(network_name.to_string_lossy().into()))
}
