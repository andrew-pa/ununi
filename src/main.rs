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

fn main() {
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
            Index::create(Path::new("./index"), schema.clone()).unwrap()
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
    

    let f = File::open("D:\\andre\\Source\\ununi\\ucd.nounihan.grouped.xml").unwrap();
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

    ixw.commit().unwrap();
    }

    index.load_searchers().unwrap();
    let s = index.searcher();
    let qp = QueryParser::new(index.schema(), vec![namef, blckf]);
    println!("READY");
    loop {
        let mut qsr = String::new();
        ::std::io::stdin().read_line(&mut qsr);
        let q = qp.parse_query(qsr.as_str().trim()).unwrap();
        let mut tpc = TopCollector::with_limit(20);
        s.search(&*q, &mut tpc).unwrap();
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
