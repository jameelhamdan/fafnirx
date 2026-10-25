//! Makes the typed queries of the master: Cornucopia writes them from `queries/*.sql`, checked against the schema the
//! migrations make, into this build's `OUT_DIR`, and `src/lib.rs` includes them. Nothing generated is in the
//! repository, so nobody reviews or merges generated code; it is made again whenever a query, a migration or this
//! script changed (and from scratch after a `cargo clean`).
//!
//! Cornucopia checks every query against a real PostgreSQL, so building needs one. In order of preference:
//!
//! 1. `FAFNIRX_CODEGEN_DATABASE_URL`: a PostgreSQL server (`postgresql://user:password@host:port`), in which a
//!    temporary database is made and dropped again.
//! 2. `FAFNIRX_TEST_DATABASE_URL`: the server the tests use anyway (its database name is ignored).
//! 3. Otherwise Docker (or Podman) starts a throwaway PostgreSQL container, which needs the image the first time.

use cornucopia::config::Config;
use std::path::{Path, PathBuf};

/// The server in an address like `postgresql://postgres:test@localhost:55432/postgres`: without a database name.
fn server_of(url: &str) -> String {
    let authority_start = url.find("://").map_or(0, |i| i + 3);
    match url[authority_start..].find('/') {
        Some(slash) => url[..authority_start + slash].to_string(),
        None => url.to_string(),
    }
}

/// The migrations, which are the schema, in the order they apply.
fn migrations(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read the migrations in {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sql"))
        .collect();
    files.sort();
    files
}

fn main() {
    let (queries, migrations_dir) = ("../../queries", "../master/migrations");
    println!("cargo:rerun-if-changed={queries}");
    println!("cargo:rerun-if-changed={migrations_dir}");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=FAFNIRX_CODEGEN_DATABASE_URL");
    println!("cargo:rerun-if-env-changed=FAFNIRX_TEST_DATABASE_URL");

    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    // Cornucopia moves what it made out of a temporary folder, which has to be on the same drive as the destination
    // (on Windows the system temporary folder often is not): so the temporary folder is next to it.
    let tmp = out.join("tmp");
    std::fs::create_dir_all(&tmp).expect("create the temporary folder");
    for var in ["TMPDIR", "TEMP", "TMP"] {
        // SAFETY: a build script is single threaded here, nothing else reads the environment.
        unsafe { std::env::set_var(var, &tmp) };
    }

    let config = Config::builder()
        .name("fafnirx-db")
        .queries(queries)
        .destination(out.join("generated"))
        .sync(true)
        .r#async(false)
        .build();
    let schema = migrations(Path::new(migrations_dir));

    let server = std::env::var("FAFNIRX_CODEGEN_DATABASE_URL")
        .or_else(|_| std::env::var("FAFNIRX_TEST_DATABASE_URL"))
        .ok()
        .map(|url| server_of(&url));
    let result = match server {
        Some(url) => {
            let temporary = format!("fafnirx_codegen_{}", std::process::id());
            cornucopia::gen_fresh(&url, &temporary, &schema, None, false, config)
        }
        None => cornucopia::gen_managed(&schema, config),
    };
    let _ = std::fs::remove_dir_all(&tmp);
    if let Err(e) = result {
        panic!(
            "cannot make the typed queries: {e}\n\n\
             Set FAFNIRX_CODEGEN_DATABASE_URL (or FAFNIRX_TEST_DATABASE_URL) to a PostgreSQL server, \
             or start Docker. See docs/development.md."
        );
    }
}
