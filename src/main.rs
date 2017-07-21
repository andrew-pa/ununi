#![windows_subsystem = "windows"]
extern crate tantivy;
extern crate xml;
extern crate winapi;
extern crate user32;
extern crate kernel32;

mod vgu;
mod app;

use winapi::*;
use user32::*;
use kernel32::*;
use std::ptr::{null,null_mut};
use std::mem::{uninitialized, transmute,size_of};



fn main() {
    unsafe { vgu::SetProcessDpiAwareness(1); }
    let mut app = app::App::new();
    unsafe {
        SetWindowLongPtrW(app.win.hndl, 0, transmute(&app));
        RegisterHotKey(app.win.hndl, 0, 1, VK_F1 as u32); //alt + f1
    }
    vgu::Window::message_loop()
}
