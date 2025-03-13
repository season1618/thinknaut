use tokio;
use regex::Regex;
use reqwest::{self, header};

use crate::data::*;
use crate::multiset::MultiSet;
use Block::*;
use Span::*;

pub fn parse_markdown(doc: &str) -> (String, List, Vec<Block>) {
    let mut parser = Parser::new(doc);
    parser.parse_markdown();
    return (parser.title, parser.toc, parser.content);
}

pub struct Parser<'a> {
    chs: &'a str,
    headers: MultiSet<String>,
    title: String,
    toc: List,
    content: Vec<Block>,
}

impl<'a> Parser<'a> {
    fn new(doc: &'a str) -> Self {
        Parser {
            chs: doc,
            headers: MultiSet::new(),
            title: String::new(),
            toc: List { ordered: true, items: Vec::new() },
            content: Vec::new(),
        }
    }

    pub fn parse_markdown(&mut self) {
        while !self.chs.is_empty() {
            let block = self.parse_block();
            match block {
                Paragraph { text } if text.0.is_empty() => {},
                _ => { self.content.push(block); },
            }
        }
    }

    fn parse_block(&mut self) -> Block {
        // header
        if self.starts_with_next("# ") {
            return self.parse_header(1);
        }
        if self.starts_with_next("## ") {
            return self.parse_header(2);
        }
        if self.starts_with_next("### ") {
            return self.parse_header(3);
        }
        if self.starts_with_next("#### ") {
            return self.parse_header(4);
        }
        if self.starts_with_next("##### ") {
            return self.parse_header(5);
        }
        if self.starts_with_next("###### ") {
            return self.parse_header(6);
        }

        // blockquote
        if self.chs.starts_with("> ") {
            return self.parse_blockquote();
        }

        // list
        if self.chs.starts_with("+ ") || self.chs.starts_with("- ") {
            return ListElement(self.parse_list(0));
        }

        // embed
        if self.starts_with_next("@[") {
            return self.parse_embed();
        }

        // math block
        if self.starts_with_next("$$") {
            return self.parse_math_block();
        }

        // code block
        if self.starts_with_next("```") {
            return self.parse_code_block();
        }

        // table
        if self.chs.starts_with("|") {
            return self.parse_table();
        }

        // paragraph
        self.parse_paragraph()
    }

    fn parse_header(&mut self, level: u32) -> Block {
        let mut header_toc = Vec::new();
        let mut header_id = String::new();

        let header = self.parse_inline();
        for span in &header.0 {
            match span {
                Link { text, .. } => {
                    for span in &text.0 {
                        header_toc.push(span.clone());
                    }
                },
                _ => header_toc.push(span.clone()),
            }
        }

        for span in &header_toc {
            match span {
                Math { math } => header_id.push_str(math),
                Code { code } => header_id.push_str(code),
                Text { text } => header_id.push_str(text),
                _ => {},
            }
        }

        // modify title or table of contents
        if level == 1 {
            self.title = header_id.clone();
        } else {
            let count = self.headers.insert(header_id.clone());
            if count > 0 {
                header_id = format!("{}-{}", &header_id, count);
            }

            let mut cur = &mut self.toc;
            for _ in 2..level {
                cur = &mut cur.items.last_mut().unwrap().list;
            }
            cur.items.push(ListItem {
                item: Inline(vec![ Link { text: Inline(header_toc), url: format!("#{}", &header_id) } ]),
                list: List { ordered: true, items: Vec::new() },
            });
        }
        Header { header, level, id: header_id }
    }

    fn parse_blockquote(&mut self) -> Block {
        let mut lines = Vec::new();
        while self.starts_with_next("> ") {
            lines.push(self.parse_inline());
        }
        Blockquote { lines }
    }

    fn parse_list(&mut self, min_indent: usize) -> List {
        let mut ordered = false;
        let mut items = Vec::new();
        while !self.chs.is_empty() {
            let chs = self.chs.trim_start_matches(' ');
            let indent = self.chs.len() - chs.len();

            if min_indent <= indent {
                self.chs = chs;

                if self.starts_with_next("- ") {
                    ordered = false;
                    items.push(ListItem {
                        item: self.parse_inline(),
                        list: self.parse_list(indent + 1),
                    });
                    continue;
                }

                if self.starts_with_next("+ ") {
                    ordered = true;
                    items.push(ListItem {
                        item: self.parse_inline(),
                        list: self.parse_list(indent + 1),
                    });
                    continue;
                }
            }
            break;
        }
        List { ordered, items }
    }

    fn parse_embed(&mut self) -> Block {
        let text = self.parse_until_trim(Self::parse_link, &["]("]);
        let url = self.text_until_trim(&[")"]).to_string();

        if url.ends_with(".png") || url.ends_with(".jpg") {
            let title = Inline(text);
            Image { title, url }
        } else {
            let (title, image, description, site_name) = get_ogp_info(&url);
            LinkCard { title, image, url, description, site_name }
        }
    }

    fn parse_math_block(&mut self) -> Block {
        let math = self.text_until_trim(&["$$"]).to_string();
        MathBlock { math }
    }

    fn parse_code_block(&mut self) -> Block {
        let lang = self.text_until_trim(&["\n", "\r\n"]).to_string();
        let code = self.text_until_trim(&["```"]).to_string();
        CodeBlock { lang, code }
    }

    fn parse_table(&mut self) -> Block {
        let mut head = Vec::new();
        let mut body = Vec::new();
        while let Some(row) = self.parse_table_row() {
            head.push(row);
        }
        while let Some(row) = self.parse_table_row() {
            body.push(row);
        }
        Table { head, body }
    }

    fn parse_table_row(&mut self) -> Option<Vec<Inline>> {
        if self.starts_with_next("-") {
            self.text_until_trim(&["\n", "\r\n"]);
            return None;
        }
        if !self.starts_with_next("|") {
            return None;
        }

        let mut row: Vec<Inline> = Vec::new();
        while !self.chs.is_empty() && !self.starts_with_newline_next() {
            let data = Inline(self.parse_until_trim(Self::parse_link, &["|"]));
            row.push(data);
        }
        Some(row)
    }

    fn parse_paragraph(&mut self) -> Block {
        Paragraph { text: self.parse_inline() }
    }

    fn parse_inline(&mut self) -> Inline {
        let mut text = Vec::new();
        while !self.chs.is_empty() && !self.starts_with_newline_next() {
            text.push(self.parse_link());
        }
        Inline(text)
    }

    fn parse_link(&mut self) -> Span {
        if self.starts_with_next("[") { // link
            let text = self.parse_until_trim(Self::parse_emph, &["]("]);
            let url = self.text_until_trim(&[")", "\n", "\r\n"]);

            let text = if text.is_empty() {
                Inline(vec![ Text { text: get_title(url) } ])
            } else { Inline(text) };

            Link { text, url: url.to_string() }
        } else {
            self.parse_emph()
        }
    }

    fn parse_emph(&mut self) -> Span {
        if self.starts_with_next("**") {
            let text = Inline(self.parse_until_trim(Self::parse_emph, &["**"]));
            Bold { text }
        } else if self.starts_with_next("__") {
            let text = Inline(self.parse_until_trim(Self::parse_emph, &["__"]));
            Ital { text }
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Span {
        // math
        if self.starts_with_next("$") {
            return self.parse_math();
        }

        // code
        if self.starts_with_next("`") {
            return self.parse_code();
        }

        // text
        self.parse_text()
    }

    fn parse_math(&mut self) -> Span {
        let math = self.text_until_trim(&["$"]);
        Math { math: math.to_string() }
    }

    fn parse_code(&mut self) -> Span {
        let code = self.text_until_trim(&["`"]);
        Code { code: code.to_string() }
    }

    fn parse_text(&mut self) -> Span {
        let text = self.text_until(&["|", "**", "__", "[", "]", "$", "`", "\n", "\r\n"]);
        Text { text: text.to_string() }
    }

    fn text_until(&mut self, terms: &[&str]) -> &str {
        let mut chs = self.chs.chars();
        let mut start = self.chs.len();
        while !chs.as_str().is_empty() {
            if terms.iter().any(|&term| chs.as_str().starts_with(term)) {
                let rest = chs.as_str();
                start -= rest.len();
                break;
            }
            chs.next();
        }
        let text = &self.chs[..start];
        self.chs = &self.chs[start..];
        text
    }

    fn text_until_trim(&mut self, terms: &[&str]) -> &str {
        let mut chs = self.chs.chars();
        let mut start = self.chs.len();
        let mut end = self.chs.len();
        while !chs.as_str().is_empty() {
            if let Some(&term) = terms.iter().find(|&term| chs.as_str().starts_with(term)) {
                let rest = chs.as_str();
                start -= rest.len();
                let rest = rest.trim_start_matches(term);
                end -= rest.len();
                break;
            }
            chs.next();
        }
        let text = &self.chs[..start];
        self.chs = &self.chs[end..];
        text
    }

    fn parse_until_trim<T>(&mut self, mut parser: impl FnMut(&mut Self) -> T, terms: &[&str]) -> Vec<T> {
        let mut res = Vec::new();
        loop {
            if let Some(term) = terms.iter().find(|&term| self.chs.starts_with(term)) {
                self.chs = self.chs.trim_start_matches(term);
                break;
            }
            res.push(parser(self));
        }
        res
    }

    fn starts_with_next(&mut self, prefix: &str) -> bool {
        if let Some(chs) = self.chs.strip_prefix(prefix) {
            self.chs = chs;
            true
        } else {
            false
        }
    }

    fn starts_with_newline_next(&mut self) -> bool {
        if let Some(chs) = self.chs.strip_prefix("\n") {
            self.chs = chs;
            true
        } else if let Some(chs) = self.chs.strip_prefix("\r\n") {
            self.chs = chs;
            true
        } else {
            false
        }
    }
}

#[tokio::main]
async fn get_title(url: &str) -> String {
    let client = reqwest::Client::new();
    let Ok(res) = client.get(url).header(header::ACCEPT, header::HeaderValue::from_str("text/html").unwrap()).send().await else {
        return String::new();
    };
    let Ok(body) = res.text().await else {
        return String::new();
    };
    let regex = Regex::new("<title>(.*)</title>").unwrap();
    if let Some(caps) = regex.captures(&body) {
        return caps[1].to_string().clone();
    }
    return String::new();
}

#[tokio::main]
async fn get_ogp_info(url: &str) -> (String, Option<String>, Option<String>, Option<String>) {
    let mut title = String::new();
    let mut image = None;
    let mut description = None;
    let mut site_name = None;

    let client = reqwest::Client::new();
    let Ok(res) = client.get(url).header(header::ACCEPT, header::HeaderValue::from_str("text/html").unwrap()).send().await else {
        return (title, image, description, site_name);
    };
    let Ok(body) = res.text().await else {
        return (title, image, description, site_name);
    };

    let regex = Regex::new("property=\"og:([^\"]*)\" content=\"([^\"]*)\"").unwrap();
    for caps in regex.captures_iter(&body) {
        match &caps[1] {
            "title" => { title = caps[2].to_string(); },
            "image" => { image = Some(caps[2].to_string()); },
            "description" => { description = Some(caps[2].to_string()); },
            "site_name" => { site_name = Some(caps[2].to_string()); },
            _ => {},
        }
    }

    if title.is_empty() {
        let regex = Regex::new("<title>(.*)</title>").unwrap();
        if let Some(caps) = regex.captures(&body) {
            title = caps[1].to_string();
        }
    }

    (title, image, description, site_name)
}