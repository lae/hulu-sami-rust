extern crate rustc_serialize as serialize;
extern crate xml;
#[macro_use]
extern crate lazy_static;
extern crate crypto;
extern crate encoding;
extern crate hyper;
extern crate url;

use std::error::Error;
use std::env;
use std::process;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write, Cursor};

use xml::reader::{EventReader, XmlEvent};
use serialize::hex::FromHex;
use encoding::{Encoding, EncoderTrap};
use encoding::all::WINDOWS_1252;
use hyper::Client;
use hyper::header::Connection;
use url::Url;

mod cryptaes;

const SYNC_KEY_ENCODED: &'static str = "4878b22e76379b55c962b18ddbc188d82299f8f52e3e698d0faf29a40ed64b21";
const SYNC_IV: &'static [u8] = b"WA7hap7AGUkevuth";

lazy_static! {
    static ref SYNC_KEY: Vec<u8> = {
        match SYNC_KEY_ENCODED.from_hex() {
            Ok(k) => k,
            Err(_) => panic!("SYNC_KEY_ENCODED is not a valid hex string")
        }
    };
}

#[derive(Debug)]
struct SubtitleLine {
    start: usize,
    end: usize,
    text: String
}

fn parser_from_http(url: &str) -> Result<(String, EventReader<Box<Read>>), Box<Error>> {
    let filename =
        try!(Url::parse(url))
        .path()
        .and_then(|path| path.last())
        .map_or(
            "whatever.srt".into(),
            |last| format!("{}.srt", last)
        );

    let mut res = try!(Client::new().get(url).header(Connection::close()).send());
    let mut body = Vec::new();

    try!(res.read_to_end(&mut body));

    Ok((filename, EventReader::new(Box::new(Cursor::new(body)))))
}

fn parser_from_file(path: &str) -> Result<(String, EventReader<Box<Read>>), Box<Error>> {
    let file = try!(File::open(path));
    let filename = if path.ends_with(".xml") {
        path.replace(".xml", ".srt")
    } else {
        format!("{}.srt", path)
    };

    Ok((filename, EventReader::new(Box::new(BufReader::new(file)))))
}

fn main() {
    let args = env::args().collect::<Vec<String>>();
    if args.len() != 2 {
        println!("{} [xml file (local or http)]", &args[0]);
        process::exit(64);
    }

    let path = &args[1];
    let result = if path.starts_with("http://") || path.starts_with("https://") {
        parser_from_http(path)
    } else {
        parser_from_file(path)
    };

    match result {
        Ok((filename, mut parser)) => write_lines(&filename, &collect_lines(&mut parser)).unwrap(),
        Err(err) => {
            println!("Failed to read {}: {}", path, err);
            process::exit(1);
        }
    }
}

fn write_lines(filename: &str, lines: &[SubtitleLine]) -> Result<(), Box<Error>> {
    let output_file = try!(File::create(filename));
    let mut output_file = BufWriter::new(output_file);

    println!("Writing SRT to {}", filename);

    for (i, line) in lines.iter().enumerate() {
        let srt_line = format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            srtime(line.start),
            srtime(line.end),
            line.text
        );
        try!(output_file.write(srt_line.as_bytes()));
    }

    try!(output_file.flush());
    Ok(()) // whatever
}

fn process_text(text: &str) -> Result<String, Box<Error>> {
    let encrypted_string = try!(text.from_hex());
    let value = try!(cryptaes::decrypt256(&encrypted_string, &*SYNC_KEY, SYNC_IV));
    let decrypted_string = try!(std::str::from_utf8(&value));

    let encoded_string = WINDOWS_1252.encode(&decrypted_string, EncoderTrap::Ignore).unwrap_or(Vec::new());
    let decoded_string = try!(std::str::from_utf8(&encoded_string));

    Ok(decoded_string.replace("<P>","").replace("</P>","").replace("<BR/>", "\n"))
}

fn collect_lines<T: Read>(parser: &mut EventReader<T>) -> Vec<SubtitleLine> {
    let mut lines = Vec::<SubtitleLine>::new();

    #[derive(Default, Debug)]
    struct State {
        in_sync: bool,
        start: Option<usize>,
        text: Option<String>
    }

    let mut parse_state: State = Default::default();

    while let Ok(event) = parser.next() {
        match event {
            XmlEvent::StartElement { name, attributes, .. } => {
                if name.local_name == "SYNC" {
                    parse_state.in_sync = true;

                    for attribute in attributes {
                        if attribute.name.local_name == "start" {
                            parse_state.start = attribute.value.parse().ok();
                            break;
                        }
                    }
                }
            },

            XmlEvent::Characters(content) => {
                if let Ok(text) = process_text(&content) {
                    if text != "" {
                        parse_state.text = Some(text)
                    }
                }
            }

            XmlEvent::EndElement { .. } => {
                if let State { in_sync: true, start: Some(start), text } = parse_state {
                    if let Some(line) = lines.last_mut() {
                        if line.end == 0 {
                            line.end = start
                        }
                    };

                    if text.is_some() {
                        lines.push(SubtitleLine { start: start, end: 0, text: text.unwrap() });
                    }
                }

                parse_state = Default::default();
            },

            XmlEvent::EndDocument => break,
            _ => ()
        }
    }

    lines
}

fn srtime(t: usize) -> String {
    let ms = t % 1000;
    let t = t / 1000;
    let s = t % 60;
    let t = t / 60;
    let m = t % 60;
    let h = m / 60;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, ms)
}
