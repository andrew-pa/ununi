extern crate tantivy;
extern crate xml;
extern crate winapi;
extern crate user32;
extern crate kernel32;

use std::path::Path;
use std::fs::File;
use std::io::BufReader;

use tantivy::Index;
use tantivy::schema::*;
use tantivy::collector::TopCollector;
use tantivy::query::QueryParser;

use xml::name::OwnedName;
use xml::attribute::OwnedAttribute;
use xml::reader::{EventReader, XmlEvent};

mod vgu;
use vgu::*;

use winapi::*;
use user32::*;
use kernel32::*;
use std::ptr::{null,null_mut};
use std::mem::transmute;

struct App {
    win: Window,
    factory: Factory,
    rt: WindowRenderTarget,
    b: Brush,
    txf: TextFactory,
    fnt: Font
}

impl App {
    fn new() -> App {
        let win = Window::new((300, 300), Some(winproc)).expect("creating window");
        let fac = Factory::new().expect("creating Direct2D factory");
        let rt = WindowRenderTarget::new(fac.clone(), &win).expect("creating HwndRenderTarget");
        let b = Brush::solid_color(rt.clone(), D2D1_COLOR_F{r:1.0, g:0.2, b:0.0, a:1.0}).expect("creating solid color brush");
        let txf = TextFactory::new().expect("creating DWrite factory");
        let fnt = Font::new(txf.clone(), String::from("Segoe UI"), DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL, 32.0).expect("creating font");
        App { 
            win, factory: fac, rt, b, txf, fnt
        }
    }

    fn paint(&mut self) {
        unsafe {
            let identity = D2D1_MATRIX_3X2_F{
                matrix:[[1.0, 0.0],
                [0.0, 1.0],
                [0.0, 0.0]]
            };

            let white = D2D1_COLOR_F{r:0.2, g:0.3, b:1.0, a:1.0};
            self.rt.BeginDraw();
            self.rt.Clear(&white);
            //self.rt.SetTransform(&identity);
            
            let r = D2D1_RECT_F{left: 32.0, right:128.0, top:32.0, bottom:128.0};
            self.rt.DrawRectangle(&r, self.b.p, 1.0, null_mut());
            self.rt.DrawText("Hello, World!".encode_utf16().collect::<Vec<u16>>().as_ptr(), 13, 
                             self.fnt.p, &r, self.b.p, D2D1_DRAW_TEXT_OPTIONS_NONE, DWRITE_MEASURING_MODE_NATURAL);

            self.rt.EndDraw(null_mut(), null_mut());
        }
    }

    fn resize(&mut self, w: u32, h: u32) {
        self.rt.resize(w, h);
    }
}

unsafe extern "system" fn winproc(win: HWND, msg: UINT, w: WPARAM, l: LPARAM) -> LRESULT {
    let papp = GetWindowLongPtrW(win, 0);
    if papp == 0 { return DefWindowProcW(win, msg, w, l); }
    let app: &mut App = transmute(papp);
    match msg {
        WM_PAINT => {
            app.paint(); 0
        },
        WM_SIZE => {
            app.resize(GET_X_LPARAM(l) as u32, GET_Y_LPARAM(l) as u32); 0
        },
        WM_HOTKEY => {
            ShowWindow(win, SW_RESTORE);
            0
        },
        WM_KEYDOWN => {
            match w as i32 {
                VK_ESCAPE => { ShowWindow(win, SW_MINIMIZE); },
                VK_PAUSE => { PostQuitMessage(0); }
                _ => {}
            }
            0
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

fn main() {
    let mut app = App::new();
    unsafe { 
        SetWindowLongPtrW(app.win.hndl, 0, transmute(&app)); 
        RegisterHotKey(app.win.hndl, 0, 4, VK_SPACE as u32); //shift + space
    }
    Window::message_loop()
}

fn old_main() {
    let mut schb = SchemaBuilder::default();
    schb.add_text_field("name", TEXT | STORED);
    schb.add_text_field("blck", TEXT | STORED);
    schb.add_u64_field("codepnt", INT_STORED);
    let schema = schb.build();
    let mut need_to_load = ::std::env::args().next().map_or(false, |_| true);
    let index = match Index::open(Path::new("./index")) {
        Ok(ix) => ix,
        Err(tantivy::Error::PathDoesNotExist(_)) => {
            need_to_load = true;
            Index::create(Path::new("./index"), schema.clone()).expect("creating search index")
        },
        Err(e) => {
            println!("error: {:?}", e);
            return;
        }
    };

    let namef = schema.get_field("name").unwrap();
    let blckf = schema.get_field("blck").unwrap();
    let cpnf  = schema.get_field("codepnt").unwrap();

    if need_to_load {
        let mut ixw = index.writer(50_000_000).unwrap();
        let f = File::open("D:\\andre\\Source\\ununi\\ucd.nounihan.grouped.xml").expect("opening Unicode XML data");
        let fbr = BufReader::new(f);
        let parser = EventReader::new(fbr);
        let mut current_block_name = String::new();
        let defatb = OwnedAttribute::new(OwnedName::local("NONE"), "0");
        for e in parser {
            match e {
                Ok(XmlEvent::StartElement { name, attributes: atrib, .. }) => {
                    match name.local_name.as_str() {
                        "group" => { 
                            current_block_name = atrib.iter().find(|&a| a.name.local_name == "blk").unwrap_or(&defatb).value.clone();
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
                Err(e) => { println!("error: {}", e); break; },
                _ => {}
            }
        }
        ixw.commit().expect("commiting index changed");
    }

    index.load_searchers().expect("loading searchers");
    let s = index.searcher();
    let qp = QueryParser::new(index.schema(), vec![namef, blckf]);
    println!("READY");
    loop {
        let mut qsr = String::new();
        ::std::io::stdin().read_line(&mut qsr);
        let q = qp.parse_query(qsr.as_str().trim()).expect("parsing search query");
        let mut tpc = TopCollector::with_limit(20);
        s.search(&*q, &mut tpc).expect("searching index");
        for da in tpc.docs() {
            let rd = s.doc(&da).unwrap();
            let cp = match rd.get_first(cpnf).unwrap() {
                &Value::U64(v) => v,
                _ => panic!("?"),
            };
            println!("[{:?}] {:?} @ {:?} = [{}]", 
                    rd.get_first(blckf).unwrap(), rd.get_first(namef).unwrap(), cp, ::std::char::from_u32(cp as u32).unwrap_or(' '));
        }
    }
}
