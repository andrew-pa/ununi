use std::path::Path;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::BufReader;
use std::error::Error;
use std::io::{Cursor, ErrorKind as IOErrorKind, copy, Seek, SeekFrom};

use tantivy::Error as TError;
use tantivy::ErrorKind as TErrorKind;
use tantivy::Index;
use tantivy::schema::*;
use tantivy::collector::TopCollector;
use tantivy::query::QueryParser;

use xml::name::OwnedName;
use xml::attribute::OwnedAttribute;
use xml::reader::{EventReader, XmlEvent};

use vgu::*;

use winapi::*;
use user32::*;
use kernel32::*;
use std::ptr::{null_mut};
use std::mem::{uninitialized, transmute,size_of};

use toml::Value as TomlValue;

/* Things left to do
 * + Restore clipboard after hijack - on demand, v0.6
 * + Recently used list - v0.7+
 * + Proper DPI handling (esp wrt multi-mon) - v0.7+
 * ✓ refactor, fix warnings - v0.5
 * ✓ Configuration file: hotkey, colors perhaps - v0.5
 * ✓ Make install not wack (automated)
 * ✓ Better Error handling
 * ✓ Cursor in search box (maybe a magnifying glass to hint that's the search box too?)
 */

#[repr(C)] #[derive(Clone,Copy,Debug)]
#[allow(non_snake_case)]
struct GUITHREADINFO {
    cbSize: DWORD,
    flags: DWORD,
    hwndActive: HWND,
    hwndFocus: HWND,
    hwndCapture: HWND,
    hwndMenuOwner: HWND,
    hwndMenuSize: HWND,
    hwndCaret: HWND,
    rcCaret: RECT
}

extern "system" { fn GetGUIThreadInfo(idThread: DWORD, lpgui: *mut GUITHREADINFO) -> BOOL; }

pub struct App {
    pub win: Window,
    factory: Factory,
    rt: WindowRenderTarget,
    b: Brush, sel_b: Brush,
    txf: TextFactory,
    fnt: Font,
    query_string: String, sel_char: usize, cursor: usize,

    namef: Field, blckf: Field, cpnf: Field,
    index: Index, 
    qpar: QueryParser, 
    last_query: Option<Vec<Document>>,

    foreground_window: Option<HWND>, ctrl_pressed: bool,

    background_color: D2D1_COLOR_F
}

fn build_index(schema: &Schema, index: &Index) -> Result<(), Box<Error>> {
    let namef = schema.get_field("name").unwrap();
    let blckf = schema.get_field("blck").unwrap();
    let cpnf  = schema.get_field("codepnt").unwrap();

    let mut ixw = index.writer(50_000_000)?;
    let fbr = BufReader::new(match File::open("./ucd.nounihan.grouped.xml") {
        Ok(f) => f,
        Err(e) => match e.kind() {
            IOErrorKind::NotFound => {
                // download latest UCD xml
                use curl::easy::Easy as Curl;
                let mut curl = Curl::new();
                let mut buffer = Vec::new();
                curl.url("http://www.unicode.org/Public/UCD/latest/ucdxml/ucd.nounihan.grouped.zip")?;
                {
                    let mut transfer = curl.transfer(); 
                    transfer.write_function(|data| { buffer.extend_from_slice(data); Ok(data.len()) })?;
                    transfer.perform()?;
                }
                use zip::read::*;
                let mut zip = ZipArchive::new(Cursor::new(&buffer))?;
                let mut archf = zip.by_index(0)?; // Unicode archives only have 1 file in them
                let mut f = OpenOptions::new().read(true).write(true).truncate(true).create(true).open("./ucd.nounihan.grouped.xml")?;
                copy(&mut archf, &mut f)?;
                f.seek(SeekFrom::Start(0))?;
                f
            },
            _ => return Err(Box::new(e))
        }
    });//.expect("opening Unicode XML data");
    let parser = EventReader::new(fbr);
    let mut current_block_name = String::new();
    let defatb = OwnedAttribute::new(OwnedName::local("NONE"), "0");
    for e in parser {
        match e {
            Ok(XmlEvent::StartElement { name, attributes: atrib, .. }) => {
                match name.local_name.as_str() {
                    "group" => {
                        current_block_name = atrib.iter().find(|&a| a.name.local_name == "blk").unwrap_or(&defatb).value.clone();
                        println!("processing {}", current_block_name);
                    },
                    "char" => {
                        let mut doc = Document::default();
                        doc.add_text(blckf, current_block_name.as_str());
                        let mut itr = atrib.iter();
                        doc.add_u64(cpnf, u64::from_str_radix(itr.find(|&a| a.name.local_name == "cp").unwrap_or(&defatb).value.as_str(), 16).unwrap());
                        doc.add_text(namef, itr.find(|&a| a.name.local_name == "na" || a.name.local_name == "na1").unwrap_or(&defatb).value.as_str());
                        ixw.add_document(doc);
                    }
                    _ => {}
                }
            },
            Err(e) => { return Err(Box::new(e)); },
            _ => {}
        }
    }
    ixw.commit()?;//.expect("commiting index changed");
    Ok(())
}

fn color_from_value(tv: &TomlValue, a: f32) -> Option<D2D1_COLOR_F> {
    tv.as_array().and_then(|v|
        v[0].as_float()
            .and_then(|r| v[1].as_float()
                              .and_then(|g| v[2].as_float()
                                                .map(|b| D2D1_COLOR_F{r: r as f32, g: g as f32, b: b as f32, a: a}))))
}

impl App {
    pub fn new(config: &Option<TomlValue>) -> Result<App, Box<Error>> {
        let mut fac = Factory::new()?;//.expect("creating Direct2D factory");
        let mut dpi: (f32, f32) = (0.0, 0.0);
        unsafe { fac.GetDesktopDpi(&mut dpi.0, &mut dpi.1); }
        let win = Window::new((((520.0) * (dpi.0 / 96.0)).ceil() as i32,
                ((520.0) * (dpi.1 / 96.0)).ceil() as i32), Some(winproc))?;//.expect("creating window");
        let rt = WindowRenderTarget::new(fac.clone(), &win)?;//.expect("creating HwndRenderTarget");

        let (main_color, sel_color, bg_color) = {
            let colors = config.as_ref().and_then(|c| c.get("colors"));
            (colors.as_ref().and_then(|c| c.get("main"))
                                   .and_then(|v| color_from_value(v, 1.0))
                                   .unwrap_or(D2D1_COLOR_F{r:0.9, g:0.9, b:0.9, a:1.0}),
             colors.as_ref().and_then(|c| c.get("highlight"))
                                   .and_then(|v| color_from_value(v, 0.8))
                                   .unwrap_or(D2D1_COLOR_F{r:0.9, g:0.8, b:0.6, a:0.8}),
             colors.as_ref().and_then(|c| c.get("background"))
                .and_then(|v| color_from_value(v,1.0)).unwrap_or(D2D1_COLOR_F{r:0.1, g:0.1, b:0.1, a:1.0}))
        };
        
        let b = Brush::solid_color(rt.clone(), main_color)?;//.expect("creating solid color brush");
        let sel_b = Brush::solid_color(rt.clone(), sel_color)?;//.expect("creating solid color brush");
        let txf = TextFactory::new().expect("creating DWrite factory");
        let fnt = Font::new(txf.clone(), String::from("Consolas"), 
                            DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL, 16.0)?;//.expect("creating font");
        let mut schb = SchemaBuilder::default();
        let namef = schb.add_text_field("name", TEXT | STORED);
        let blckf = schb.add_text_field("blck", TEXT | STORED);
        let cpnf = schb.add_u64_field("codepnt", INT_STORED);
        let schema = schb.build();
        let index = match Index::open(Path::new("./index")) {
            Ok(ix) => ix,
            Err(TError(TErrorKind::PathDoesNotExist(_), _)) => {
                fs::create_dir("./index")?;//.expect("create index directory");
                let ix = Index::create(Path::new("./index"), schema.clone())?;//.expect("creating search index");
                if let Err(e) = build_index(&schema, &ix) {
                    fs::remove_dir_all("./index")?;
                    return Err(e);
                }
                ix
            },
            Err(e) => {
                return Err(Box::new(e));//panic!("creating index: {:?}", e);
            }
        };
        index.load_searchers()?;//.expect("loading searchers");
        Ok(App {
            win, factory: fac, rt, b, sel_b, txf, fnt, query_string: String::from(""), sel_char: 0, cursor: 0,
            namef, blckf, cpnf, index: index,
            qpar: QueryParser::new(schema.clone(), vec![namef, blckf]),
            background_color: bg_color,
            last_query: None, foreground_window: None, ctrl_pressed: false
        })
    }

    unsafe fn paint(&mut self) {
        self.rt.BeginDraw();
        self.rt.Clear(&self.background_color);
        //self.rt.SetTransform(&identity);

        { //draw frame
            let mut r = D2D1_RECT_F{left: 0.0, right:520.0, top:0.0, bottom:520.0};
            self.rt.DrawRectangle(&r, self.sel_b.p, 1.0, null_mut());
        }

        // draw the query 'textbox'
        let query_layout = TextLayout::new(self.txf.clone(), &self.query_string, &self.fnt, 512.0, 32.0).expect("create query string layout");
        let mut r = D2D1_RECT_F{left: 8.0, right:512.0, top:8.0, bottom:32.0};
        self.rt.DrawRectangle(&r, self.b.p, 1.0, null_mut());
        r.left += 2.0; r.top += 2.0;
        self.rt.DrawTextLayout(D2D1_POINT_2F{x: r.left, y: r.top}, query_layout.p, self.b.p, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
        let mut cb = query_layout.char_bounds(self.cursor);
        cb.left += r.left; cb.top += r.top;
        cb.right += r.left; cb.bottom += r.top;
        if cb.left == cb.right { cb.right += 8.0; }
        self.rt.FillRectangle(&cb, self.sel_b.p);


        // draw the query results
        r.top += 28.0; r.bottom += 28.0;
        match self.last_query {
            Some(ref das) => {
                let sel_char = self.sel_char;
                for (rd,sel) in das.iter().zip((0..).map(|i| i == sel_char)) {
                    let cp = rd.get_first(self.cpnf).unwrap().u64_value();
                    let entry = format!("{}: {} - {}", ::std::char::from_u32(cp as u32).unwrap_or(' '),
                            rd.get_first(self.namef).unwrap().text(),
                            rd.get_first(self.blckf).unwrap().text());
                    let entry16 = entry.encode_utf16().collect::<Vec<u16>>();
                    self.rt.DrawText(entry16.as_ptr(), entry16.len() as u32,
                                     self.fnt.p, &r, self.b.p, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT | D2D1_DRAW_TEXT_OPTIONS_CLIP, DWRITE_MEASURING_MODE_NATURAL);

                    if sel { self.rt.DrawRectangle(&r, self.sel_b.p, 1.0, null_mut()); }
                    r.top += 24.0; r.bottom += 24.0;
                }
            }, 
            None => {} 
        }
        if self.ctrl_pressed {
            self.rt.DrawLine(D2D1_POINT_2F{x: 0.0, y:0.0}, D2D1_POINT_2F{x:800.0,y:0.0}, self.sel_b.p, 1.0, null_mut());
        }
        self.rt.EndDraw(null_mut(), null_mut());
    }

    fn resize(&mut self, w: u32, h: u32) {
        self.rt.resize(w, h);
    }

    fn update_query(&mut self) {
        let q = match self.qpar.parse_query(self.query_string.as_str().trim()) {
            Ok(v) => v,
            Err(_) => { return; }
        };
        let s = self.index.searcher(); let mut tpc = TopCollector::with_limit(20);
        s.search(&*q, &mut tpc).expect("searching index");
        self.last_query = Some(tpc.docs().iter().map(|da| s.doc(&da).unwrap()).collect());
        self.sel_char = 0;
    }

    unsafe fn hotkey(&mut self) {
        let mut gti: GUITHREADINFO = uninitialized();
        gti.cbSize = size_of::<GUITHREADINFO>() as u32;
        GetGUIThreadInfo(0, &mut gti);
        self.foreground_window = if !gti.hwndFocus.is_null() { Some(gti.hwndFocus) } else { None };

        self.update_query(); 

        let mut frc: RECT = uninitialized();
        GetWindowRect(gti.hwndFocus, &mut frc);
        SetWindowPos(self.win.hndl, transmute(-1 as isize), frc.left + gti.rcCaret.left, frc.top + gti.rcCaret.bottom+4, 0, 0, SWP_NOSIZE | SWP_SHOWWINDOW);
        ShowWindow(self.win.hndl, SW_RESTORE);
        SetForegroundWindow(self.win.hndl);
    }
    unsafe fn char_event(&mut self, w: u16) {
        let s = String::from_utf16(&[w]).unwrap();
        if !s.chars().next().unwrap().is_control() {
            self.query_string.insert_str(self.cursor, &s);
            self.cursor+=1;
            self.update_query();
        }
    }

    unsafe fn send_char(&mut self, fw: HWND, cp: char, use_clipboard: bool) {
        if use_clipboard {
            OpenClipboard(self.win.hndl);
            EmptyClipboard();
            let global_text = GlobalAlloc(0x0042, 6);
            let tcopy: &mut [u16; 3] = transmute(GlobalLock(global_text));
            cp.encode_utf16(tcopy);
            GlobalUnlock(global_text);
            SetClipboardData(CF_UNICODETEXT, global_text);
            CloseClipboard();
            SetForegroundWindow(fw);
            keybd_event(VK_CONTROL as u8, 0, 0, 0);
            keybd_event(b'V', 0, 0, 0);
            keybd_event(b'V', 0, KEYEVENTF_KEYUP, 0);
            keybd_event(VK_CONTROL as u8, 0, KEYEVENTF_KEYUP, 0);
        } else {
            let mut tbuf = [0u16, 2];
            let chb = cp.encode_utf16(&mut tbuf);
            SetForegroundWindow(fw);
            for c in chb {
                PostMessageW(fw, WM_CHAR, *c as WPARAM, 1);
            }
        }
    }

    unsafe fn send_selected_char(&mut self, use_clipboard: bool) -> LRESULT {
        self.query_string = String::new(); self.cursor = 0;
        if self.foreground_window != None && self.last_query != None {
            let fw = self.foreground_window.unwrap();
            let cp = self.last_query.as_ref()
                .and_then(|lq| lq[self.sel_char].get_first(self.cpnf).map(Value::u64_value))
                .and_then(|cp| ::std::char::from_u32(cp as u32)); 
            self.send_char(fw, cp.expect("obtain char from query"), use_clipboard);
            self.foreground_window = None;
        }
        self.ctrl_pressed = false;
        ShowWindow(self.win.hndl, SW_HIDE);
        0
    }

    unsafe fn keydown(&mut self, w: WPARAM) -> LRESULT {
        match w as i32 {
            VK_BACK => {
                if self.cursor == 0 { 0 } else {
                if self.cursor == self.query_string.len()  { self.query_string.pop(); }
                else { self.query_string.remove(self.cursor); }
                self.cursor-=1;
                self.update_query(); 0 }
            },
            VK_ESCAPE => { 
                self.query_string = String::new();
                self.sel_char = 0;
                self.cursor = 0;
                ShowWindow(self.win.hndl, SW_HIDE); 0 
            },
            VK_CONTROL => {self.ctrl_pressed = true; 0},
            VK_RETURN => { let ctlp = self.ctrl_pressed; self.send_selected_char(!ctlp) },
            VK_UP => { if self.sel_char > 0 { self.sel_char -= 1; } 0 },
            VK_DOWN => {
                match self.last_query.as_ref() {
                    Some(q) => { if self.sel_char < q.len() { self.sel_char += 1; } },
                    None => {}
                }
                0
            },
            VK_LEFT => { if self.cursor > 0 { self.cursor -= 1; } 0 },
            VK_RIGHT => { if self.cursor < self.query_string.len() { self.cursor += 1; } 0 },
            VK_PAUSE => { PostQuitMessage(0); 0 }
            _ => 1
        }
    }
}

unsafe extern "system" fn winproc(win: HWND, msg: UINT, w: WPARAM, l: LPARAM) -> LRESULT {
    let papp = GetWindowLongPtrW(win, 0);
    if papp == 0 { return DefWindowProcW(win, msg, w, l); }
    let app: &mut App = transmute(papp);
    match msg {
        WM_PAINT => {
            app.paint(); 1
        },
        WM_SIZE => {
            app.resize(GET_X_LPARAM(l) as u32, GET_Y_LPARAM(l) as u32); 0
        },
        WM_HOTKEY => {
           app.hotkey(); 0
        },
        WM_CHAR => {
            app.char_event(w as u16); 1
        },
        WM_KEYDOWN => {
            app.keydown(w)
        },
        WM_KEYUP => {
            match w as i32 {
                VK_CONTROL => app.ctrl_pressed = false,
                _ => {}
            }; 0
        },
        WM_CREATE => {
            SetWindowLongPtrW(win, 0, 0); 0
        },
        WM_DESTROY => {
            PostQuitMessage(0); 1
        }
        _ => DefWindowProcW(win, msg, w, l)
    }
}
