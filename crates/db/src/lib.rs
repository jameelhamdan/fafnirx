//! The typed queries of the master, one function and one row type per query in `queries/*.sql`.
//!
//! Everything is made by `build.rs` with Cornucopia into this build's output folder, and none of it is in the
//! repository. The generated code is the root of this crate (its own modules and `crate::` paths expect that), so it is
//! included here as it is.

include!(concat!(env!("OUT_DIR"), "/generated/src/lib.rs"));
