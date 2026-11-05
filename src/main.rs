#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod bag;
mod board;
mod bot;
mod config;
mod desktop;
mod finesse;
mod game;
mod highscores;
mod input;
mod piece;
mod scoring;
mod settings;
mod training;
mod ui;

fn main() {
    desktop::run();
}
