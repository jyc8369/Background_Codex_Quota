#![cfg_attr(windows, windows_subsystem = "windows")]

mod api;
mod auth;
mod config;
mod control;
mod quota;
mod runtime;
mod windows;

fn main() {
    if let Err(error) = runtime::run() {
        eprintln!("backend stopped with an error: {error}");
        std::process::exit(1);
    }
}
