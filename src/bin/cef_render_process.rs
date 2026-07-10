// CEF render-process helper for the react-hud-cef feature. Chromium launches this binary for its
// renderer/gpu/utility subprocesses; cef_offscreen finds it as `<host exe>-cef` next to
// decentra-bevy, so it builds and ships like any other workspace binary.

// Same console policy as the host binary: a console-subsystem child spawned by the
// GUI-subsystem host has no console to inherit, so windows pops a visible one for every
// Chromium subprocess (gpu, utility, renderers). In `console` builds the children attach
// to the host's console instead of opening their own.
#![cfg_attr(not(feature = "console"), windows_subsystem = "windows")]

fn main() {
    // Die with the parent: CEF subprocesses normally exit when the browser process's IPC channel
    // drops, but a hard-killed (SIGKILL) browser never runs cef shutdown and can leave helpers
    // lingering; once we're reparented (ppid 1 = orphaned) the browser is gone, so exit.
    #[cfg(unix)]
    std::thread::spawn(|| loop {
        if std::os::unix::process::parent_id() == 1 {
            std::process::exit(0);
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    });

    cef_offscreen::prelude::execute_render_process();
}
