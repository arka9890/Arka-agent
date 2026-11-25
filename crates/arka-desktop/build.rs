//! Suntikkan hash commit ke binary untuk tab "Tentang" & laporan bug.
//!
//! CI boleh mengisi `ARKA_GIT_COMMIT` sendiri; kalau tidak ada, build.rs
//! membacanya dari repo git lokal. Tidak ada git (mis. source tarball) →
//! string kosong, dan `build_commit()` menampilkan "tidak diketahui".

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=ARKA_GIT_COMMIT");

    let dari_env = std::env::var("ARKA_GIT_COMMIT")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let commit = dari_env
        .or_else(|| {
            std::process::Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_default();

    println!("cargo:rustc-env=ARKA_GIT_COMMIT={commit}");
}
