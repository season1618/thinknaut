use std::io::{self, Write};
use std::fs::File;
use chrono::{Local, Datelike, Timelike};

use crate::data::*;

use Block::*;
use Span::*;
use Prim::*;
use Elem::*;

pub fn gen_html(dest: &mut File, title: &String, toc: &List, content: &Vec<Block>, template: &Vec<Elem>) -> Result<(), io::Error> {
    let mut codegen = CodeGen::new(dest);
    codegen.gen_html(title, toc, content, template)
}

struct CodeGen<'a> {
    dest: &'a mut File,
}

impl<'a> CodeGen<'a> {
    fn new(dest: &'a mut File) -> Self {
        CodeGen { dest }
    }

    fn gen_html(&mut self, title: &String, toc: &List, content: &Vec<Block>, template: &Vec<Elem>) -> Result<(), io::Error> {
        let datetime = Local::now();
        for chunk in template {
            match chunk {
                Title => write!(self.dest, "{}", title)?,
                Year   => write!(self.dest, "{:04}", datetime.year())?,
                Month  => write!(self.dest, "{:02}", datetime.month())?,
                Day    => write!(self.dest, "{:02}", datetime.day())?,
                Hour   => write!(self.dest, "{:02}", datetime.hour())?,
                Minute => write!(self.dest, "{:02}", datetime.minute())?,
                Second => write!(self.dest, "{:02}", datetime.second())?,
                Toc(indent) => self.gen_toc(toc, *indent)?,
                Content(indent) => self.gen_content(content, *indent)?,
                Str(text) => write!(self.dest, "{}", text)?,
            }
        }
        Ok(())
    }

    fn gen_toc(&mut self, toc: &List, indent: usize) -> Result<(), io::Error> {
        writeln!(self.dest)?;
        self.gen_list(&toc, indent)
    }

    fn gen_content(&mut self, content: &Vec<Block>, indent: usize) -> Result<(), io::Error> {
        writeln!(self.dest)?;
        for block in content {
            match block {
                Header { prims, level, id } => self.gen_header(prims, level, id, indent)?,
                Blockquote { lines } => self.gen_blockquote(lines, indent)?,
                ListElement(list) => self.gen_list(list, indent)?,
                Table { head, body } => self.gen_table(head, body, indent)?,
                Image { title, url } => self.gen_image(title, url, indent)?,
                LinkCard { title, image, url, description, site_name } => self.gen_link_card(title, image, url, description, site_name, indent)?,
                MathBlock { math } => self.gen_math_block(math, indent)?,
                CodeBlock { lang, code } => self.gen_code_block(lang, code, indent)?,
                Paragraph { spans } => self.gen_paragraph(spans, indent)?,
            }
        }
        Ok(())
    }

    fn gen_header(&mut self, prims: &Vec<Prim>, level: &u32, id: &String, indent: usize) -> Result<(), io::Error> {
        write!(self.dest, "{:>indent$}<h{} id=\"{}\">", " ", *level, *id)?;
        self.gen_prims(prims)?;
        writeln!(self.dest, "</h{}>", *level)
    }

    fn gen_blockquote(&mut self, lines: &Vec<Vec<Span>>, indent: usize) -> Result<(), io::Error> {
        writeln!(self.dest, "{:>indent$}<blockquote>", " ")?;
        for spans in lines {
            write!(self.dest, "{:>indent$}  <p>", " ")?;
            self.gen_spans(spans)?;
            writeln!(self.dest, "</p>")?;
        }
        writeln!(self.dest, "{:>indent$}</blockquote>", " ")
    }

    fn gen_list(&mut self, list: &List, indent: usize) -> Result<(), io::Error> {
        if list.items.is_empty() {
            return Ok(());
        }

        writeln!(self.dest, "{:>indent$}<{}>", " ", if list.ordered { "ol" } else { "ul" })?;
        for item in &list.items {
            writeln!(self.dest, "{:>indent$}  <li>", " ")?;
            
            write!(self.dest, "{:>indent$}    ", " ")?;
            self.gen_spans(&item.spans)?;
            writeln!(self.dest)?;
            self.gen_list(&item.list, indent + 4)?;
            
            writeln!(self.dest, "{:>indent$}  </li>", " ")?;
        }
        writeln!(self.dest, "{:>indent$}</{}>", " ", if list.ordered { "ol" } else { "ul" })
    }

    fn gen_image(&mut self, title: &Vec<Prim>, url: &String, indent: usize) -> Result<(), io::Error> {
        writeln!(self.dest, "{:>indent$}<div class=\"image\">", " ")?;
        writeln!(self.dest, "{:>indent$}  <img src=\"{}\">", " ", *url)?;
        write!(self.dest, "{:>indent$}  <p class=\"caption\">", " ")?;
        self.gen_prims(title)?;
        writeln!(self.dest, "</p>")?;
        writeln!(self.dest, "{:>indent$}</div>", " ")
    }

    fn gen_link_card(&mut self, title: &String, image: &Option<String>, url: &String, description: &Option<String>, site_name: &Option<String>, indent: usize) -> Result<(), io::Error> {
        writeln!(self.dest, "{:>indent$}<div class=\"linkcard\"><a class=\"linkcard-link\" href=\"{}\">", "", url)?;
        writeln!(self.dest, "{:>indent$}  <div class=\"linkcard-text\">", "")?;
        writeln!(self.dest, "{:>indent$}    <h3 class=\"linkcard-title\">{}</h3>", "", title)?;
        if let Some(desc) = description {
            writeln!(self.dest, "{:>indent$}    <p class=\"linkcard-description\">{}</p>", "", desc)?;
        }
        writeln!(self.dest, "{:>indent$}    <img  class=\"linkcard-favicon\" src=\"http://www.google.com/s2/favicons?domain={}\"><span  class=\"linkcard-sitename\">{}</span>", "", url, site_name.clone().unwrap_or(url.clone()))?;
        writeln!(self.dest, "{:>indent$}  </div>", "")?;
        if let Some(img) = image {
            writeln!(self.dest, "{:>indent$}  <img class=\"linkcard-image\" src=\"{}\">", "", img)?;
        }
        writeln!(self.dest, "{:>indent$}</a></div>", "")
    }

    fn gen_table(&mut self, head: &Vec<Vec<String>>, body: &Vec<Vec<String>>, indent: usize) -> Result<(), io::Error> {
        writeln!(self.dest, "{:>indent$}<table>", " ")?;

        writeln!(self.dest, "{:>indent$}  <thead>", " ")?;
        for row in head {
            writeln!(self.dest, "{:>indent$}    <tr>", " ")?;
            for data in row {
                writeln!(self.dest, "{:>indent$}      <td>{}</td>", " ", *data)?;
            }
            writeln!(self.dest, "{:>indent$}    </tr>", " ")?;
        }
        writeln!(self.dest, "{:>indent$}  </thead>", " ")?;
        
        writeln!(self.dest, "{:>indent$}  <tbody>", " ")?;
        for row in body {
            writeln!(self.dest, "{:>indent$}    <tr>", " ")?;
            for data in row {
                writeln!(self.dest, "{:>indent$}      <td>{}</td>", " ", *data)?;
            }
            writeln!(self.dest, "{:>indent$}    </tr>", " ")?;
        }
        writeln!(self.dest, "{:>indent$}  </tbody>", " ")?;
        
        writeln!(self.dest, "{:>indent$}</table>", " ")
    }

    fn gen_math_block(&mut self, math: &String, indent: usize) -> Result<(), io::Error> {
        writeln!(self.dest, "{:>indent$}<p>\\[{}\\]</p>", " ", math)
    }

    fn gen_code_block(&mut self, lang: &String, code: &String, indent: usize) -> Result<(), io::Error> {
        write!(self.dest, "{:>indent$}<pre><code class=\"language-{}\">", " ", if lang == "" { "plaintext" } else { lang })?;
        write!(self.dest, "{}", code)?;
        writeln!(self.dest, "</code></pre>")
    }

    fn gen_paragraph(&mut self, spans: &Vec<Span>, indent: usize) -> Result<(), io::Error> {
        write!(self.dest, "{:>indent$}<p>", " ")?;
        self.gen_spans(spans)?;
        writeln!(self.dest, "</p>")
    }

    fn gen_spans(&mut self, spans: &Vec<Span>) -> Result<(), io::Error> {
        for span in spans {
            match span {
                Bold { text } => self.gen_bold(text)?,
                Ital { text } => self.gen_ital(text)?,
                PrimElem(prim) => self.gen_primary(prim)?,
            }
        }
        Ok(())
    }

    fn gen_bold(&mut self, text: &Vec<Span>) -> Result<(), io::Error> {
        write!(self.dest, "<strong>")?;
        self.gen_spans(text)?;
        write!(self.dest, "</strong>")
    }

    fn gen_ital(&mut self, text: &Vec<Span>) -> Result<(), io::Error> {
        write!(self.dest, "<em>")?;
        self.gen_spans(text)?;
        write!(self.dest, "</em>")
    }

    fn gen_prims(&mut self, prims: &Vec<Prim>) -> Result<(), io::Error> {
        for prim in prims {
            self.gen_primary(prim)?;
        }
        Ok(())
    }

    fn gen_primary(&mut self, prim: &Prim) -> Result<(), io::Error> {
        match prim {
            Link { text, url } => {
                write!(self.dest, "<a href=\"{}\">", *url)?;
                self.gen_prims(text)?;
                write!(self.dest, "</a>")
            },
            Math { math } => write!(self.dest, "\\({}\\)", *math),
            Code { code } => write!(self.dest, "<code>{}</code>", *code),
            Text { text } => write!(self.dest, "{}", text),
        }
    }
}