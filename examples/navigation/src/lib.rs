mod app;
#[cfg(feature = "browser-tests")]
mod browser_tests;
pub mod pages;
pub mod routes;
include!(env!("FUSOR_MODULE"));
