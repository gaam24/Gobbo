//! Small macros shared by the hook modules.

/// The original function a hook replaces, or `fallback` when the hook table or this detour is missing.
///
/// A hook whose export was missing is never installed, so its detour function cannot be reached through the detour. The
/// fallback keeps a defensive path for a partial install and for a direct call during initialization: the caller returns
/// the API's failure value instead of panicking. `fallback` must have the type of the enclosing function's return.
macro_rules! original {
    ($field:ident, $fallback:expr) => {
        match crate::state::hooks().and_then(|hooks| hooks.$field.as_ref()) {
            Some(original) => original,
            None => return $fallback,
        }
    };
}
