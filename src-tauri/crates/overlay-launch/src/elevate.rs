//! Restarting Gobbo with administrator rights. The shell shows the UAC consent; nothing here bypasses it. The app crate
//! forbids unsafe, so this one Win32 call lives in the launcher, which already calls Win32.

use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::ptr::{null, null_mut};

use gobbo_overlay_core::wide;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CANCELLED};
use windows_sys::Win32::Security::{
    GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::quote_argument;

/// Why a restart as administrator did not start.
#[derive(Debug)]
pub enum ElevateError {
    /// The user closed the UAC prompt. This is a choice, not a failure to report.
    Cancelled,
    /// The shell could not start the new copy, for example because the user has no administrator credentials.
    Io(io::Error),
}

/// Starts `exe` with administrator rights and the given arguments. The call returns after the UAC prompt: `Ok` means
/// the new process exists, `Cancelled` means the user said no. It does not wait for the new process.
pub fn restart_elevated(exe: &Path, args: &[String]) -> Result<(), ElevateError> {
    let verb = wide(OsStr::new("runas"));
    let file = wide(exe.as_os_str());
    let parameters = parameters(args);
    let parameters_ptr = if parameters.is_empty() {
        null()
    } else {
        parameters.as_ptr()
    };
    // A `runas` start otherwise works in the system folder, because the shell does not carry the caller's working
    // directory. Gobbo's files are addressed through its executable, but keeping the folder avoids a surprise for a
    // relative path a provider or a definition left behind.
    let directory = std::env::current_dir()
        .ok()
        .map(|path| wide(path.as_os_str()));
    let directory_ptr = directory.as_ref().map_or_else(null, |path| path.as_ptr());

    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: parameters_ptr,
        lpDirectory: directory_ptr,
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };

    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(ERROR_CANCELLED as i32) {
            Err(ElevateError::Cancelled)
        } else {
            Err(ElevateError::Io(error))
        };
    }

    if !info.hProcess.is_null() {
        // SAFETY: the shell returned a process handle and hands its ownership to the caller.
        unsafe { CloseHandle(info.hProcess) };
    }

    Ok(())
}

/// The arguments as one `lpParameters` string. The same quoting the launcher uses for `CreateProcessW` keeps spaces and
/// quotes intact.
fn parameters(args: &[String]) -> Vec<u16> {
    let mut line = Vec::new();

    for arg in args {
        if !line.is_empty() {
            line.push(u16::from(b' '));
        }

        line.extend(quote_argument(&arg.encode_utf16().collect::<Vec<_>>()));
    }

    line.push(0);
    line
}

/// Whether this process runs with administrator rights. An in-app update refuses to write program files with the
/// elevated token; a failure to read the token means "not elevated".
pub fn is_elevated() -> bool {
    let mut token = null_mut();

    // SAFETY: the process handle is the current process, and `token` is a valid out pointer.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return false;
    }

    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut length = 0u32;
    // SAFETY: the buffer is the size of `TOKEN_ELEVATION`, which matches the `TokenElevation` class.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            std::ptr::from_mut(&mut elevation).cast(),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut length,
        )
    };
    // SAFETY: the token handle was opened above and is closed only here.
    unsafe { CloseHandle(token) };

    ok != 0 && elevation.TokenIsElevated != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_stay_separate() {
        let encoded = parameters(&["--wait-for-parent".to_string(), "42".to_string()]);
        let text = String::from_utf16_lossy(&encoded[..encoded.len() - 1]);

        assert_eq!(text, "--wait-for-parent \"42\"");
    }

    #[test]
    fn an_argument_with_a_space_is_quoted() {
        let encoded = parameters(&["C:\\Games\\A B".to_string()]);
        let text = String::from_utf16_lossy(&encoded[..encoded.len() - 1]);

        assert_eq!(text, "\"C:\\Games\\A B\"");
    }
}
