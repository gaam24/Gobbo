//! The macro that declares the short commands which run one use case for one game.

/// Defines the commands that run one use case for one game, the shape most commands have. Each takes the game id and
/// its own arguments, and runs on a blocking thread:
///
/// - `query name(args) -> Answer = work;` answers with what `work` returns;
/// - `change name(args) = work;` answers with the game's fresh status after `work`.
///
/// `work` is a function of `(&Data, &str)`, or `|root, game| body` with the arguments in scope. A command that needs
/// more (the app handle, a channel, another state) is written out as a plain function.
macro_rules! game_commands {
    () => {};
    (
        $(#[$doc:meta])*
        query $name:ident($($arg:ident: $ty:ty),* $(,)?) -> $out:ty = |$root:ident, $game:ident| $body:expr;
        $($rest:tt)*
    ) => {
        $(#[$doc])*
        #[tauri::command]
        #[specta::specta]
        pub async fn $name(
            data: tauri::State<'_, $crate::context::Data>,
            game: String,
            $($arg: $ty),*
        ) -> Result<$out, $crate::error::AppError> {
            $crate::commands::helpers::for_game(data, game, move |$root, $game| $body).await
        }

        $crate::commands::macros::game_commands!($($rest)*);
    };
    ($(#[$doc:meta])* query $name:ident() -> $out:ty = $work:path; $($rest:tt)*) => {
        $(#[$doc])*
        #[tauri::command]
        #[specta::specta]
        pub async fn $name(
            data: tauri::State<'_, $crate::context::Data>,
            game: String,
        ) -> Result<$out, $crate::error::AppError> {
            $crate::commands::helpers::for_game(data, game, $work).await
        }

        $crate::commands::macros::game_commands!($($rest)*);
    };
    (
        $(#[$doc:meta])*
        change $name:ident($($arg:ident: $ty:ty),* $(,)?) = |$root:ident, $game:ident| $body:expr;
        $($rest:tt)*
    ) => {
        $(#[$doc])*
        #[tauri::command]
        #[specta::specta]
        pub async fn $name(
            data: tauri::State<'_, $crate::context::Data>,
            game: String,
            $($arg: $ty),*
        ) -> Result<$crate::status::views::Status, $crate::error::AppError> {
            $crate::commands::helpers::change(data, game, move |$root, $game| $body).await
        }

        $crate::commands::macros::game_commands!($($rest)*);
    };
    ($(#[$doc:meta])* change $name:ident() = $work:path; $($rest:tt)*) => {
        $(#[$doc])*
        #[tauri::command]
        #[specta::specta]
        pub async fn $name(
            data: tauri::State<'_, $crate::context::Data>,
            game: String,
        ) -> Result<$crate::status::views::Status, $crate::error::AppError> {
            $crate::commands::helpers::change(data, game, $work).await
        }

        $crate::commands::macros::game_commands!($($rest)*);
    };
}

/// The short per-game command macro, used by the area command modules.
pub(crate) use game_commands;
