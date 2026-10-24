//! Opens an agent's sign-in page on this computer. The headless CLI often only
//! names the URL; the host is what can open it.

/// Opens `url` with this computer's browser. Anything but an http(s) URL is ignored.
pub fn open(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return;
    }
    let mut command = if cfg!(windows) {
        let mut command = std::process::Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    } else {
        std::process::Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" })
    };
    if command
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_err()
    {
        tracing::warn!("couldn't open the sign-in page");
    }
}
