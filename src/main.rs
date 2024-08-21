use clap::Parser;
use clap_derive::{Subcommand, ValueEnum};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, io::{Read, Write}, path::PathBuf};

const LINKU_API: &str = "https://api.linku.la/v1/words";

//ends = ['.',',',':','!','?']
//  "usage_category"
//  "word"

// 1 byte per word
// get input file

// number=word in alpha order

//cats={'common', 'core', 'uncommon', 'obscure'}
#[derive(Deserialize, Serialize,Clone, Copy,PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Cat {
    Common,
    Core,
    Uncommon,
    Obscure,
}
#[derive(Deserialize, Serialize, Clone,PartialEq, Eq)]
struct Word {
    word: String,
    #[serde(rename = "usage_category")]
    cat: Cat,
}
fn request_words() -> HashMap<String, Word> {
    let mut r = reqwest::blocking::get(LINKU_API).unwrap();
    let mut buf = Vec::new();
    r.read_to_end(&mut buf).unwrap();
    let words: HashMap<String, Word> = serde_json::from_slice(&buf).unwrap();
    return words;
}

struct Words {
    to_tpc: HashMap<WordOrSpecial, u8>,
    from_tpc: HashMap<u8, WordOrSpecial>,
}

fn get_words() -> HashMap<String, Word> {
    let cache = std::path::Path::new("words.json");
    if cache.exists() {
        //then parse
        let f = std::fs::read(cache).unwrap();
        return serde_json::from_slice(&f).unwrap();
    } else {
        let words = request_words();
        let w = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(cache)
            .unwrap();
        serde_json::to_writer(w, &words).unwrap();
        return words;
    }
}
#[derive(Hash,PartialEq, Eq,Clone)]
enum Special {
    StartUppercase,
    BeginAscii,
    EndAscii
}
#[derive(Hash,PartialEq, Eq,Clone)]
enum WordOrSpecial {
    Word(String),
    Special(Special)
}
fn gen_conversions(words: HashMap<String, Word>) -> Words {
    let mut w: Vec<Word> = words.into_values().collect();
    w.sort_by_key(|s: &Word| -> String { s.word.clone() });
    
    let e = w.into_iter().enumerate()
        .map(|(i,v)| {(i as u8, WordOrSpecial::Word(v.word))});
    let mut from_tpc: HashMap<u8, WordOrSpecial> = HashMap::from_iter(e);
    from_tpc.insert(0xfd, WordOrSpecial::Word(".".to_string()));
    from_tpc.insert(0xfc, WordOrSpecial::Word(",".to_string()));
    from_tpc.insert(0xfb, WordOrSpecial::Word(":".to_string()));
    from_tpc.insert(0xfa, WordOrSpecial::Word("!".to_string()));
    from_tpc.insert(0xf9, WordOrSpecial::Word("?".to_string()));

    from_tpc.insert(0xff, WordOrSpecial::Special(Special::BeginAscii));
    from_tpc.insert(0xfe, WordOrSpecial::Special(Special::EndAscii));
    from_tpc.insert(0xf8, WordOrSpecial::Special(Special::StartUppercase));

    let to = 
    from_tpc.iter().map(|(k,v)| -> (WordOrSpecial, u8) {(v.clone(),*k)});
    let to_tpc:HashMap<WordOrSpecial, u8> = HashMap::from_iter(to); 
    Words { to_tpc, from_tpc }
}

fn compress(conv:&Words,text:&String) -> Vec<u8> {
    let words = text.split_whitespace();
    const PUNCT: [char;5] = ['.',',',':','!','?'];
    let mut end:Option<u8> = None;
    let mut out: Vec<u8> = Vec::new();
    // header: [TPC]ompress, version
    let tpc: String = "TPC".to_string();
    out.append(&mut tpc.into_bytes());
    out.push(env!("CARGO_PKG_VERSION_MAJOR").parse().unwrap());
    out.push(env!("CARGO_PKG_VERSION_MINOR").parse().unwrap());
    out.push(env!("CARGO_PKG_VERSION_PATCH").parse().unwrap());

    for word in words {
        let mut bare_word: String = word.to_string();
        if let Some(x) = word.chars().last() {
            if PUNCT.contains(&x) {
                end=Some(conv.to_tpc[&WordOrSpecial::Word(x.to_string())]);
                bare_word=bare_word.strip_suffix(x).unwrap().to_string();
            }
        }
        if let Some(c) = bare_word.chars().next() {
            if c.is_uppercase() {
                out.push(conv.to_tpc[&WordOrSpecial::Special(Special::StartUppercase)]);
                bare_word=bare_word.to_lowercase();
            }
        }
        out.push(conv.to_tpc[&WordOrSpecial::Word(bare_word)]);
        if let Some(end) = end {
            out.push(end)
        }
    }
    out
}

#[derive(Clone, Copy,ValueEnum)]
enum DeComp {
    Decompress,
    Compress
}
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// Turn debugging information on
    #[arg(long)]
    debug: bool,

    #[arg(value_enum)]
    decomp:DeComp,

    #[arg()]
    file: PathBuf,

    
    #[arg(short,long,default_value="out.tpc")]
    out: PathBuf,
}

fn main() {
    let words = gen_conversions(get_words());
    let a = Args::parse();
    match a.decomp {
        DeComp::Compress => {
            let mut f = std::fs::OpenOptions::new()
                .create(false)
                .read(true)
                .write(false)
                .open(a.file).unwrap();
            let mut text = String::new();
            f.read_to_string(&mut text).unwrap();

            let mut outf = std::fs::OpenOptions::new()
                .create(true)
                .read(false)
                .write(true)
                .append(false)
                .open(a.out).unwrap();
            outf.write(&compress(&words, &text)).unwrap();
        }
        DeComp::Decompress => {}
    };
    println!("Hello, world!");
}
