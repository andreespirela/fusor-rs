mod api;
mod app;
#[cfg(feature = "browser-tests")]
mod browser_tests;
mod session;
mod views;
include!(env!("FUSOR_MODULE"));
