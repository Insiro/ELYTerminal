pub mod app;
pub mod atomic;
pub mod blur;
pub mod docs;
#[cfg(windows)]
pub mod explorer;
pub mod files;
pub mod git;
pub mod i18n;
pub mod identity;
pub mod interface;
pub mod menus;
pub mod pet;
pub mod profiles;
pub mod projects;
pub mod sessions;
pub mod settings;
pub mod term;
#[cfg(test)]
mod testlog;
#[cfg(all(test, target_os = "linux"))]
mod testx11;
pub mod themes;
pub mod updates;
pub mod usage;
pub mod workspace;
