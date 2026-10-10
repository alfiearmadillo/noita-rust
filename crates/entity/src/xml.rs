//! A forgiving XML reader for game files. The reference's parser accepts things strict XML does
//! not: comments inside a tag, repeated attributes (the last value wins here) and `--` inside
//! comments. Only elements and attributes are kept; text is ignored.

use crate::Node;

pub fn parse(text: &str) -> Result<Node, String> {
    let mut p = Parser {
        s: text.as_bytes(),
        text,
        i: 0,
    };
    p.skip_misc()?;
    let root = p.element()?.ok_or("no root element")?;
    Ok(root)
}

struct Parser<'a> {
    s: &'a [u8],
    text: &'a str,
    i: usize,
}

impl Parser<'_> {
    fn err(&self, msg: &str) -> String {
        let line = self.s[..self.i.min(self.s.len())]
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
            + 1;
        format!("{msg} (line {line})")
    }

    fn starts(&self, pat: &str) -> bool {
        self.s[self.i..].starts_with(pat.as_bytes())
    }

    fn skip_until(&mut self, pat: &str) -> Result<(), String> {
        match self.text[self.i..].find(pat) {
            Some(p) => {
                self.i += p + pat.len();
                Ok(())
            }
            None => Err(self.err(&format!("unterminated, expected {pat}"))),
        }
    }

    fn skip_ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    /// Skips declarations, comments and text up to the next element start.
    fn skip_misc(&mut self) -> Result<(), String> {
        loop {
            while self.i < self.s.len() && self.s[self.i] != b'<' {
                self.i += 1;
            }
            if self.starts("<?") {
                self.skip_until("?>")?;
            } else if self.starts("<!--") {
                self.skip_until("-->")?;
            } else if self.starts("<![CDATA[") {
                self.skip_until("]]>")?;
            } else if self.starts("<!") {
                self.skip_until(">")?;
            } else {
                return Ok(());
            }
        }
    }

    fn name(&mut self) -> String {
        let start = self.i;
        while self.i < self.s.len() {
            let b = self.s[self.i];
            if b.is_ascii_whitespace() || matches!(b, b'/' | b'>' | b'=' | b'<') {
                break;
            }
            self.i += 1;
        }
        self.text[start..self.i].to_owned()
    }

    /// Parses the element at the cursor (`<`), or `None` at an end tag or the end of input.
    fn element(&mut self) -> Result<Option<Node>, String> {
        self.skip_misc()?;
        if self.i >= self.s.len() || self.starts("</") {
            return Ok(None);
        }
        self.i += 1; // '<'
        let mut node = Node {
            tag: self.name(),
            ..Node::default()
        };
        if node.tag.is_empty() {
            return Err(self.err("empty tag name"));
        }
        loop {
            self.skip_ws();
            if self.i >= self.s.len() {
                return Err(self.err("unterminated tag"));
            }
            if self.starts("<!--") {
                self.skip_until("-->")?;
            } else if self.starts("/>") {
                self.i += 2;
                return Ok(Some(node));
            } else if self.s[self.i] == b'>' {
                self.i += 1;
                break;
            } else {
                let key = self.name();
                if key.is_empty() {
                    return Err(self.err("bad attribute"));
                }
                self.skip_ws();
                if self.s.get(self.i) == Some(&b'=') {
                    self.i += 1;
                    self.skip_ws();
                    let value = self.value()?;
                    node.set_attr(&key, &value);
                }
            }
        }
        while let Some(child) = self.element()? {
            node.children.push(child);
        }
        if self.starts("</") {
            self.skip_until(">")?;
        }
        Ok(Some(node))
    }

    fn value(&mut self) -> Result<String, String> {
        let quote = match self.s.get(self.i) {
            Some(&q @ (b'"' | b'\'')) => q,
            _ => return Err(self.err("attribute value is not quoted")),
        };
        self.i += 1;
        let start = self.i;
        while self.i < self.s.len() && self.s[self.i] != quote {
            self.i += 1;
        }
        if self.i >= self.s.len() {
            return Err(self.err("unterminated attribute value"));
        }
        let raw = &self.text[start..self.i];
        self.i += 1;
        Ok(unescape(raw))
    }
}

fn unescape(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_owned();
    }
    raw.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}
