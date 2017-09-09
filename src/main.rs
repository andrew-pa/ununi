#![windows_subsystem = "windows"]
extern crate tantivy;
extern crate xml;
extern crate winapi;
extern crate user32;
extern crate kernel32;
extern crate advapi32;
extern crate curl;
extern crate zip;
extern crate toml;

mod vgu;
mod app;

use winapi::*;
use user32::*;
use kernel32::*;
use advapi32::*;
use std::ptr::{null_mut};
use std::mem::{uninitialized, transmute};
use std::io::{Read, ErrorKind as IOErrorKind};
use std::env;
use toml::Value as TomlValue;

fn display_error<E: ::std::error::Error>(e: &E) {
    let mut text = format!("Error: {}", e).encode_utf16().collect::<Vec<u16>>();
    text.push(0); text.push(0);
    unsafe { MessageBoxW(null_mut(), text.as_ptr(), null_mut(), MB_ICONERROR) };
}

fn main() {
    unsafe {
        vgu::SetProcessDpiAwareness(1);
    }

    // check to see if %APPDATA% directory exists, if not create it, then change directories there
    let appdata_path = ::std::path::Path::new(&env::var("APPDATA").expect("%APPDATA% env var")).join("ununi");
    if !appdata_path.exists() {
        ::std::fs::create_dir_all(&appdata_path).expect("create %APPDATA%\\ununi directory");
    }
    env::set_current_dir(appdata_path).expect("change directory into %APPDATA%\\ununi");

    // check to see if running with flag /S
    if !::std::env::args().any(|s| s == "/S") {
        // if not → ask user if want to run at startup
        let mut text = "Do you want ununi to run at startup?".encode_utf16().collect::<Vec<u16>>();
        text.push(0); text.push(0);
        let res = unsafe { MessageBoxW(null_mut(), text.as_ptr(), [0u16, 0u16].as_ptr(), MB_YESNO) };
        if res == 6 /* IDYES */ {
        //          create registry key (running with /S flag)
            let subkey = "Software\\Microsoft\\Windows\\CurrentVersion\\Run".encode_utf16().chain((0..).take(2)).collect::<Vec<u16>>();
            let mut module_path = [0u16; 256];
            let path_len = unsafe { GetModuleFileNameW(null_mut(), module_path.as_mut_ptr(), 256) } as usize;
            let cmdline = String::from_utf16(&module_path[0..path_len]).expect("decode module path") + " /S";
            let value = cmdline.encode_utf16().chain((0..).take(2)).collect::<Vec<u16>>();
            unsafe {
                let mut key: HKEY = uninitialized();
                RegCreateKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, null_mut(), 0, KEY_WRITE, null_mut(), transmute(&mut key), null_mut());
                RegSetValueExW(key, [b'u' as u16, 0u16, 0u16].as_ptr(), 0, REG_SZ, value.as_ptr() as *mut u8, (value.len()*2) as u32);
                RegCloseKey(key);
            }
        }
    }

    let config = match ::std::fs::File::open("config.toml") {
        Ok(mut f) => {
            let mut config_text = String::new();
            f.read_to_string(&mut config_text);
            match config_text.parse::<TomlValue>() {
                Ok(c) => Some(c),
                Err(e) => {
                    display_error(&e);
                    return;
                }
            }
        },
        Err(e) => match e.kind() {
            IOErrorKind::NotFound => None,
            _ => {
                display_error(&e);
                return;
            }
        }
    };

    let mut app = match app::App::new(&config) {
        Ok(v) => v,
        Err(e) => {
            let mut text = format!("Error: {}", e).encode_utf16().collect::<Vec<u16>>();
            text.push(0); text.push(0);
            unsafe { MessageBoxW(null_mut(), text.as_ptr(), null_mut(), MB_ICONERROR) };
            return;
        }
    };
    unsafe {
        SetWindowLongPtrW(app.win.hndl, 0, transmute(&app));

        // default is alt+f1
        let hotkey_table = config.as_ref().and_then(|c| c.get("hotkey"));
        let hotkey_mod = hotkey_table.and_then(|c| c.get("mod"))
            .and_then(TomlValue::as_str)
            .map_or(1, |v| match v {
                "alt" => 1,
                "ctrl" => 2,
                "shift" => 4,
                "windows" => 8,
                _ => 1
            });
        let hotkey_key = hotkey_table.and_then(|c| c.get("key"))
            .and_then(TomlValue::as_integer)
            .map_or(VK_F1 as u32, |v| v as u32);
        RegisterHotKey(app.win.hndl, 0, hotkey_mod, hotkey_key);
    }
    vgu::Window::message_loop()
}
