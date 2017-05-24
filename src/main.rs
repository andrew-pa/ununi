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
    fnt: Font,
    query_string: String,

    schema: Schema,
    namef: Field, blckf: Field, cpnf: Field,
    index: Index, 
    qpar: QueryParser, 
    last_query: Option<Vec<Document>>
}

fn build_index(schema: &Schema, index: &Index) {
    let namef = schema.get_field("name").unwrap();
    let blckf = schema.get_field("blck").unwrap();
    let cpnf  = schema.get_field("codepnt").unwrap();

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

impl App {
    fn new() -> App {
        let win = Window::new((800, 800), Some(winproc)).expect("creating window");
        let fac = Factory::new().expect("creating Direct2D factory");
        let rt = WindowRenderTarget::new(fac.clone(), &win).expect("creating HwndRenderTarget");
        let b = Brush::solid_color(rt.clone(), D2D1_COLOR_F{r:0.9, g:0.9, b:0.9, a:1.0}).expect("creating solid color brush");
        let txf = TextFactory::new().expect("creating DWrite factory");
        let fnt = Font::new(txf.clone(), String::from("Consolas"), 
                            DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL, 16.0).expect("creating font");
        let mut schb = SchemaBuilder::default();
        let namef = schb.add_text_field("name", TEXT | STORED);
        let blckf = schb.add_text_field("blck", TEXT | STORED);
        let cpnf = schb.add_u64_field("codepnt", INT_STORED);
        let schema = schb.build();
        let index = match Index::open(Path::new("./index")) {
            Ok(ix) => ix,
            Err(tantivy::Error::PathDoesNotExist(_)) => {
                let ix = Index::create(Path::new("./index"), schema.clone()).expect("creating search index");
                build_index(&schema, &ix);
                ix
            },
            Err(e) => {
                panic!("creating index: {:?}", e);
            }
        };
        index.load_searchers().expect("loading searchers");
        App {
            win, factory: fac, rt, b, txf, fnt, query_string: String::from(""),
            schema: schema.clone(), namef, blckf, cpnf, index: index,
            qpar: QueryParser::new(schema.clone(), vec![namef, blckf]), last_query: None
        }
    }

    fn paint(&mut self) {
        unsafe {
            let bg = D2D1_COLOR_F{r:0.1, g:0.1, b:0.1, a:1.0};
            self.rt.BeginDraw();
            self.rt.Clear(&bg);
            //self.rt.SetTransform(&identity);

            let mut r = D2D1_RECT_F{left: 8.0, right:512.0, top:8.0, bottom:32.0};
            self.rt.DrawRectangle(&r, self.b.p, 1.0, null_mut());
            let s = self.query_string.encode_utf16().collect::<Vec<u16>>();
            self.rt.DrawText(s.as_ptr(), s.len() as u32,
                             self.fnt.p, &r, self.b.p, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, DWRITE_MEASURING_MODE_NATURAL);
            r.top += 28.0; r.bottom += 28.0;
            match self.last_query {
                Some(ref das) => {
                    for rd in das {
                        let cp = rd.get_first(self.cpnf).unwrap().u64_value();
                        let S = /*format!("[{:?}] {:?} @ {:?} = [{}]",
                                 rd.get_first(self.blckf).unwrap(), 
                                 rd.get_first(self.namef).unwrap(), cp, ::std::char::from_u32(cp as u32).unwrap_or(' '));*/
                            format!("{}: {} - {}", ::std::char::from_u32(cp as u32).unwrap_or(' '),
                                rd.get_first(self.namef).unwrap().text(),
                                rd.get_first(self.blckf).unwrap().text());
                        let s = S.encode_utf16().collect::<Vec<u16>>();
                        self.rt.DrawText(s.as_ptr(), s.len() as u32,
                             self.fnt.p, &r, self.b.p, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, DWRITE_MEASURING_MODE_NATURAL);
                        r.top += 24.0; r.bottom += 24.0;
                    }
                }, 
                None => {} 
            }


            self.rt.EndDraw(null_mut(), null_mut());
        }
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
            ShowWindow(win, SW_RESTORE);
            0
        },
        WM_CHAR => {
            let s = String::from_utf16(&[w as u16]).unwrap();
            if !s.chars().next().unwrap().is_control() {
                app.query_string += &s;
                app.update_query();
            }
            1
        },
        WM_KEYDOWN => {
            match w as i32 {
                VK_BACK => { app.query_string.pop(); app.update_query(); 0 },
                VK_ESCAPE => { ShowWindow(win, SW_HIDE); 0 },
                VK_PAUSE => { PostQuitMessage(0); 0 }
                _ => 1
            }
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
        RegisterHotKey(app.win.hndl, 0, 1, VK_F1 as u32); //shift + space
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
