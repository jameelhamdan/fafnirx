//! `text_enum!`: an enum whose values the database stores as text. It owns the spelling, so that a column is read and written in
//! one place and every `match` on it is checked by the compiler.
//!
//! ```ignore
//! text_enum! {
//!     /// How it went.
//!     pub enum Status { Ok = "ok", Failed = "failed" }
//! }
//! Status::Ok.as_str() == "ok";  Status::parse("failed") == Some(Status::Failed);
//! ```

macro_rules! text_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident { $($(#[$vmeta:meta])* $variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        $vis enum $name {
            $($(#[$vmeta])* $variant),+
        }

        impl $name {
            /// Every value, in the order they are declared.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The text the database stores.
            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $text),+
                }
            }

            /// `None` for text this version does not write.
            pub fn parse(s: &str) -> Option<$name> {
                Self::ALL.iter().copied().find(|v| v.as_str() == s)
            }
        }
    };
}
