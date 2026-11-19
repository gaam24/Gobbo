//! Windows 11 Efficiency mode (`EcoQoS`). The call lowers the process priority and asks Windows to run it on the most
//! efficient cores and clock; Task Manager then shows the green leaf. Other systems have no equivalent.

/// Puts this process into Efficiency mode, or back to normal. A failure (for example Windows 10 without `EcoQoS`) is
/// not fatal: the app keeps running at its normal priority.
#[cfg(windows)]
pub fn set_efficiency_mode(enabled: bool) -> bool {
    win32_ecoqos::process::toggle_efficiency_mode(std::process::id(), Some(enabled)).is_ok()
}

/// Other systems have no Efficiency mode.
#[cfg(not(windows))]
pub const fn set_efficiency_mode(_enabled: bool) -> bool {
    false
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_platform_without_efficiency_mode_reports_that_it_did_nothing() {
        #[cfg(not(windows))]
        assert!(!super::set_efficiency_mode(true));
    }
}
