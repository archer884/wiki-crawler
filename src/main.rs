use std::{
    fmt,
    fs::{File},
    io::{self, BufRead, BufReader},
    ops::Not,
    process,
};

use clap::Parser;
use regex::Regex;
use serde::Deserialize;
use serde_xml_rs as xml;

#[derive(Debug, Parser)]
struct Args {
    path: String,
}

#[derive(Deserialize)]
struct Page {
    title: String,
    revision: Vec<Revision>,
}

impl Page {
    fn text(&self) -> Option<&str> {
        let candidate = &self.revision.first()?.text;
        candidate
            .starts_with("#REDIRECT")
            .not()
            .then_some(candidate)
    }
}

#[derive(Deserialize)]
struct Revision {
    text: String,
}

impl fmt::Debug for Page {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Page").field("title", &self.title).finish()
    }
}

struct PageBuffer<T> {
    reader: T,
}

impl<T> PageBuffer<T>
where
    T: BufRead,
{
    fn new(reader: T) -> Self {
        Self { reader }
    }
}

impl<T> Iterator for PageBuffer<T>
where
    T: BufRead,
{
    type Item = io::Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut take = false;
        let mut buf = String::new();

        for line in self.reader.by_ref().lines() {
            let text = match line {
                Ok(text) => text,
                Err(e) => return Some(Err(e)),
            };

            if text.trim() == "<page>" {
                take = true;
                buf += &text;
                buf += "\n";
                continue;
            }

            if text.trim() == "</page>" {
                buf += &text;
                buf += "\n";
                return Some(Ok(buf));
            }

            if take {
                buf += &text;
                buf += "\n";
            }
        }

        buf.is_empty().not().then_some(Ok(buf))
    }
}

#[derive(Debug)]
struct TextFilter {
    braces: Regex,
    parens: Regex,
    source: Regex,
}

impl TextFilter {
    fn new() -> Self {
        Self {
            braces: Regex::new(r#"(?sm)\{\{.*?\}\}"#).unwrap(),
            parens: Regex::new(r#"\(.+?\)"#).unwrap(),
            source: Regex::new(r#"<ref>.+?</ref>"#).unwrap(),
        }
    }

    fn filter(&self, text: &str) -> String {
        let text = self.parens.replace_all(&text, "");
        let text = self.braces.replace_all(&text, "");
        let text = self.source.replace_all(&text, "");
        text.into()
    }
}

#[derive(Debug)]
struct LinkExtractor {
    expr: Regex,
}

impl LinkExtractor {
    fn new() -> Self {
        Self {
            expr: Regex::new(r#"\[\[([^|]+?)(\|.+)?\]\]"#).unwrap(),
        }
    }

    fn extract<'a>(&self, text: &'a str) -> Option<&'a str> {
        let paragraphs = text
            .lines()
            .filter(|&text| text.starts_with(|u: char| u.is_alphanumeric() || u == '\''));

        let candidates = paragraphs.flat_map(|paragraph| {
            self.expr
                .captures_iter(paragraph)
                .filter_map(|cx| cx.get(1).map(|cx| cx.as_str()))
        });

        for candidate in candidates {
            // if candidate.starts_with("File:") {
            //     continue;
            // }

            return Some(candidate);
        }

        None
    }
}

fn main() {
    if let Err(e) = run(&Args::parse()) {
        eprintln!("{e}");
        process::exit(1);
    }
}

fn run(args: &Args) -> anyhow::Result<()> {
    let tf = TextFilter::new();
    let ex = LinkExtractor::new();

    let file = File::open(&args.path).map(BufReader::new)?;
    let pages = PageBuffer::new(file)
        .filter_map(|text| xml::from_str::<Page>(&text.ok()?).ok())
        .filter(|page| !page.title.ends_with("(disambiguation)"))
        .filter_map(|page| {
            ex.extract(&tf.filter(page.text()?))
                .map(|link| (page.title, link.to_string()))
        });

    for (title, link) in pages {
        println!("{title} -> {link}")
    }

    Ok(())
}
