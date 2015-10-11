extern crate rustc_serialize as serialize;
extern crate xml;
#[macro_use]
extern crate lazy_static;
extern crate crypto;
extern crate encoding;
extern crate hyper;
extern crate url;

use std::env;
use std::process;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};

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

fn main() {
    let args = env::args().collect::<Vec<String>>();
    if args.len() != 2 {
        println!("{} [xml file (local or http)]", &args[0]);
        process::exit(64);
    }
    let hulu_xml = &args[1];
    if hulu_xml.starts_with("http://") || hulu_xml.starts_with("https://") {
        let filename = match Url::parse(hulu_xml) {
            Ok(url) => {
                url.path()
                .and_then(|path| path.last())
                .map_or(
                    "whatever.srt".into(),
                    |last| format!("{}.srt", last)
                )
            }
            Err(err) => {
                println!("Couldn't parse URL: {:?}", err);
                process::exit(1);
            }
        };
        let client = Client::new();
        let mut res = client.get(hulu_xml).header(Connection::close()).send().unwrap();
        let mut body = String::new();
        res.read_to_string(&mut body).unwrap();
        let mut parser = EventReader::from_str(&body);
        write_lines(filename, collect_lines(&mut parser));
    } else {
        let file = match File::open(hulu_xml) {
            Ok(file) => BufReader::new(file),
            Err(err) => {
                println!("Failed to open {}: {}", hulu_xml, err.to_string());
                process::exit(1);
            }
        };
        let hulu_srt = match hulu_xml.ends_with(".xml") {
            true => hulu_xml.replace(".xml", ".srt"),
            false => format!("{}.srt", hulu_xml)
        };
        let mut parser = EventReader::new(file);
        write_lines(hulu_srt, collect_lines(&mut parser));
    }
}

fn write_lines(filename: String, lines: Vec<SubtitleLine>) {
    let mut output_file = match File::create(&filename) {
        Ok(file) => BufWriter::new(file),
        Err(err) => {
            println!("Failed to open {} for writing: {}", filename, err.to_string());
            process::exit(1);
        }
    };
    println!("Writing SRT to {}", filename);
    for (i, line) in lines.iter().enumerate() {
        let srt_line = format!("{}\n{} --> {}\n{}\n\n",
                                i+1,
                                srtime(line.start),
                                srtime(line.end),
                                line.text);
        output_file.write(srt_line.as_bytes()).unwrap();
    }
    output_file.flush().unwrap();
}

fn collect_lines<T: Read>(parser: &mut EventReader<T>) -> Vec<SubtitleLine> {
    //let mut parser = EventReader::new(reader);
    let mut lines: Vec<SubtitleLine> = vec![];
    while let Ok(event) = parser.next() {
        match event {
            XmlEvent::StartElement { name, attributes, .. } => {
                if name.local_name == "SYNC" {
                    let mut line = SubtitleLine { start: 0, end: 0, text: String::new() };
                    for attribute in attributes {
                        if attribute.name.local_name == "start" {
                            line.start = match attribute.value.parse() {
                                Ok(time) => time,
                                Err(_) => 0
                            }
                        }
                    }
                    while let Ok(sync_event) = parser.next() {
                        match sync_event {
                            XmlEvent::EndElement { .. } => {
                                if lines.len() > 0 {
                                    let mut last_line: &mut SubtitleLine = match lines.last_mut() {
                                        Some(l) => l,
                                        None => break
                                    };
                                    if last_line.end == 0 {
                                        last_line.end = line.start;
                                    }
                                }
                                if line.text != "" {
                                    lines.push(line)
                                } 
                                break
                            }
                            XmlEvent::Characters(content) => {
                                let encrypted_string = content.from_hex().unwrap();
                                let value = cryptaes::decrypt256(&encrypted_string, &*SYNC_KEY, SYNC_IV).unwrap();
                                let decrypted_string = std::str::from_utf8(&value).unwrap();
                                let encoded_string = WINDOWS_1252.encode(&decrypted_string, EncoderTrap::Ignore).unwrap();
                                let decoded_string = std::str::from_utf8(&encoded_string).unwrap();
                                let cleaned_string = decoded_string.replace("<P>","").replace("</P>","").replace("<BR/>", "\n");
                                line.text = cleaned_string.to_owned();
                            }
                            _ => break
                        }
                    }
                }
            }
            XmlEvent::EndDocument => break,
            _ => { }
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
