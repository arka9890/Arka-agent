//! `arka-core` — logika Arka Agent tanpa UI dan tanpa JNI.
//!
//! Crate ini dipakai dua platform:
//!  * **Android** lewat `arka_android` (shim JNI + proot);
//!  * **Windows** lewat `arka_desktop` (egui + PowerShell + DPAPI).
//!
//! Karena itu tidak boleh ada di sini: `#[no_mangle] extern "system"`, `jni`,
//! `std::os::unix`, atau string yang menganggap platform selalu Ubuntu.
//! Prompt & path platform dipilih lewat [`paths`] dan [`prompt`].

pub mod agent;
pub mod ai;
pub mod db;
pub mod events;
pub mod file;
pub mod memory;
pub mod paths;
pub mod prompt;
pub mod provider;
pub mod secret;
pub mod session;
pub mod settings;
pub mod shell;
pub mod skill;
pub mod terminal;
pub mod ui;

pub use events::{Events, NoopEvents};
pub use paths::{AppPaths, Platform};
pub use shell::{ExecOutcome, Shell};
