#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod ephemeris;

fn main() {
    app::run();
}
