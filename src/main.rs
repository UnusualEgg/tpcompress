use clap::Parser;
use clap_derive::ValueEnum;
use core::panic;
use log::{debug, trace};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Write},
    path::PathBuf,
};

const LINKU_API: &str = "https://api.linku.la/v1/words";
// 1 byte per word
//cats={'common', 'core', 'uncommon', 'obscure'}


#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Cat {
    Common,
    Core,
    Uncommon,
    Obscure,
}
#[derive(Deserialize, Serialize, Clone, PartialEq, Eq)]
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
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
enum Special {
    StartUppercase = 0xf8,
    BeginAscii = 0xff,
    EndAscii = 0xfe,
}
#[derive(Hash, PartialEq, Eq, Clone)]
enum WordOrSpecial {
    Word(String),
    Special(Special),
}
fn gen_conversions(words: HashMap<String, Word>) -> Words {
    let mut w: Vec<Word> = words.into_values().collect();
    w.sort_by_key(|s: &Word| -> String { s.word.clone() });

    let e = w
        .into_iter()
        .enumerate()
        .map(|(i, v)| (i as u8, WordOrSpecial::Word(v.word)));
    let mut from_tpc: HashMap<u8, WordOrSpecial> = HashMap::from_iter(e);
    from_tpc.insert(0xfd, WordOrSpecial::Word(".".to_string()));
    from_tpc.insert(0xfc, WordOrSpecial::Word(",".to_string()));
    from_tpc.insert(0xfb, WordOrSpecial::Word(":".to_string()));
    from_tpc.insert(0xfa, WordOrSpecial::Word("!".to_string()));
    from_tpc.insert(0xf9, WordOrSpecial::Word("?".to_string()));
    from_tpc.insert(0xf7, WordOrSpecial::Word("\n".to_string()));
    from_tpc.insert(0xf6, WordOrSpecial::Word("\t".to_string()));

    from_tpc.insert(0xff, WordOrSpecial::Special(Special::BeginAscii));
    from_tpc.insert(0xfe, WordOrSpecial::Special(Special::EndAscii));
    from_tpc.insert(0xf8, WordOrSpecial::Special(Special::StartUppercase));

    let to = from_tpc
        .iter()
        .map(|(k, v)| -> (WordOrSpecial, u8) { (v.clone(), *k) });
    let to_tpc: HashMap<WordOrSpecial, u8> = HashMap::from_iter(to);
    Words { to_tpc, from_tpc }
}

const PUNCT: [char; 7] = ['.', ',', ':', '!', '?','\n','\t'];

fn get_punct(s:&str,conv: &Words) -> Option<(Vec<u8>,String)> {
    let mut v = Vec::new();
    let mut c: VecDeque<char> = s.chars().collect();
    let mut copy: Option<String> = None;
    while let Some(x) = c.back() {
        if !PUNCT.contains(&x) {
            break;
        }
        if copy.is_none() {
            copy=Some(s.to_string());
        }
        v.push(conv.to_tpc[&WordOrSpecial::Word(x.to_string())]);
        if let Some(inner) = copy {
            copy = Some(inner.strip_suffix([*x]).unwrap().to_string());
        }
        c.pop_back();
    }
    if v.len()>0 {
        return Some((v,copy.unwrap()));
    }
    return None;
}
fn compress(conv: &Words, text: &String) -> Vec<u8> {
    let words: Vec<&str> = text.split(' ').collect();
    let mut end: Vec<u8> = Vec::new();
    let mut out: Vec<u8> = Vec::new();
    // header: [TPC]ompress, version
    let tpc: String = "TPC".to_string();
    out.append(&mut tpc.into_bytes());
    out.push(env!("CARGO_PKG_VERSION_MAJOR").parse().unwrap());
    out.push(env!("CARGO_PKG_VERSION_MINOR").parse().unwrap());
    out.push(env!("CARGO_PKG_VERSION_PATCH").parse().unwrap());

    let mut i = 0;
    let len = words.len();
    while i<len {
        let word = words[i];
        //won't include any punctuation or whitespace
        let mut bare_word: String = word.to_string();
        
        if let Some((new_end,new_word)) = get_punct(word, conv) {
            bare_word= new_word;
            end = new_end;
        }
        
        //check if this is proper (capitalize)
        if let Some(c) = bare_word.chars().next() {
            if c.is_uppercase() {
                out.push(conv.to_tpc[&WordOrSpecial::Special(Special::StartUppercase)]);
                bare_word = bare_word.to_lowercase();
            }
        }
        if let Some(bare_word) = conv.to_tpc.get(&WordOrSpecial::Word(bare_word)) {
            out.push(*bare_word);
        } else {
            out.push(Special::BeginAscii as u8);
            out.extend(word.as_bytes());
            out.push(Special::EndAscii as u8);
        }
        if !end.is_empty() {
            out.extend(&end);
        }
        end.clear();
        i+=1;
    }
    out
}
fn verify(i: &mut usize, data: &Vec<u8>) {
    let header: &[u8] = data
        .get(0..6)
        .expect("expected TPC header. Maybe wrong file type");
    let tpc = header.get(0..3).unwrap();
    if tpc != "TPC".as_bytes() {
        panic!("expected TPC in header. Maybe wrong file type");
    }
    *i += header.len();
}
//source: https://stackoverflow.com/a/38406885
fn first_uppercase(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().chain(c).collect(),
    }
}
fn decompress(conv: &Words, data: &Vec<u8>) -> String {
    let mut out = String::new();
    let mut i = 0;
    let len = data.len();
    //skip header
    debug!("verifying header");
    verify(&mut i, data);
    let begin = i;

    debug!("decompressing");
    while i < len {
        let byte = &data[i];
        if let Some(word) = conv.from_tpc.get(byte) {
            match word {
                WordOrSpecial::Word(word) => {
                    if i!=begin&&!(word.len()>0&&PUNCT.contains(&word.chars().next().unwrap())) {
                        out.push(' ');
                    }
                    out.push_str(word);
                }
                WordOrSpecial::Special(Special::StartUppercase) => {
                    const EXPECTED_WORD: &str = "expected word after uppercase byte";
                    i += 1;
                    let next = data.get(i).expect(EXPECTED_WORD);
                    let word = conv.from_tpc.get(next).expect(EXPECTED_WORD);
                    match word {
                        WordOrSpecial::Special(x) => {
                            panic!("{} but got {:?}", EXPECTED_WORD, x);
                        }
                        WordOrSpecial::Word(word) => {
                            if i!=begin&&!(word.len()>0&&PUNCT.contains(&word.chars().next().unwrap())) {
                                out.push(' ');
                            }
                            let copy = first_uppercase(word);
                            out.push_str(&copy);
                        }
                    }
                }
                WordOrSpecial::Special(Special::BeginAscii) => {
                    if i!=begin {
                        out.push(' ');
                    }
                    i += 1;
                    let mut next = data.get(i).expect("Expected ASCII Character");
                    while next != &(Special::EndAscii as u8) {
                        let bytes = [*next];
                        let s = std::str::from_utf8(&bytes).unwrap();
                        out.push_str(s);
                        i += 1;
                        next = data.get(i).expect("Expected ASCII Character");
                    }
                }
                WordOrSpecial::Special(Special::EndAscii) => {
                    panic!("Found EndAscii before BeginAscii");
                }
            }
            i += 1;
        }
    }
    out
}

#[derive(Clone, Copy, ValueEnum, Debug)]
enum DeComp {
    Decompress,
    Compress,
}
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Turn debugging information on
    #[arg(long)]
    debug: bool,

    #[arg(value_enum)]
    decomp: DeComp,

    #[arg()]
    file: PathBuf,

    #[arg(short, long, default_value = "out.tpc")]
    out: PathBuf,
}

fn main() {
    colog::init();
    let words = gen_conversions(get_words());
    let a = Args::parse();

    trace!("Args: {:?}", a);

    let mut f = std::fs::OpenOptions::new()
        .create(false)
        .read(true)
        .write(false)
        .open(a.file)
        .unwrap();
    let mut outf = std::fs::OpenOptions::new()
        .create(true)
        .read(false)
        .write(true)
        .append(false)
        .open(a.out)
        .unwrap();
    match a.decomp {
        DeComp::Compress => {
            let mut text = String::new();
            f.read_to_string(&mut text).unwrap();

            let out = compress(&words, &text);
            outf.write(&out).unwrap();
            let in_size = text.len() as f32;
            let out_size = out.len() as f32;
            let change: f32 = in_size / out_size;
            let percent = (change * 100.) as u32;
            println!("Deflated {}%", percent);
        }
        DeComp::Decompress => {
            let mut data = Vec::new();
            f.read_to_end(&mut data).unwrap();

            let out_string = decompress(&words, &data);
            let out = out_string.as_bytes();
            outf.write(out).unwrap();
            let in_size = data.len() as f32;
            let out_size = out.len() as f32;
            let change: f32 = in_size / out_size;
            let percent = (change * 100.) as u32;
            println!("Inflated {}%", percent);
        }
    };
    println!("Done!");
}
