//! This module holds the libc calls the tool needs.
//! It gives each call a safe interface.

/// Lets a write to a pipe with no reader end the run quietly.
pub fn restore_sigpipe() {
    // SAFETY: no thread exists yet, and SIG_DFL installs no handler code.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
}

/// Answers whether the process runs as root.
pub(crate) fn privileged() -> bool {
    // SAFETY: geteuid takes no argument and always succeeds.
    (unsafe { libc::geteuid() }) == 0
}
