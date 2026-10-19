//! Steam's text KeyValues ("VDF"), as in libraryfolders.vdf and the
//! appmanifest_*.acf files: quoted keys and values, `{ }` blocks, `//` comments.

use anyhow::{bail, Result};

#[derive(Clone, Debug, PartialEq)]
pub enum Vdf {
    Text(String),
    Block(Vec<(String, Vdf)>),
}

impl Vdf {
    /// A key's value in a block; Steam's keys don't care about case.
    pub fn get(&self, key: &str) -> Option<&Vdf> {
        match self {
            Vdf::Block(entries) => entries.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            Vdf::Text(_) => None,
        }
    }

    pub fn text(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Vdf::Text(text) => Some(text),
            Vdf::Block(_) => None,
        }
    }

    /// A block's entries, in file order.
    pub fn entries(&self) -> &[(String, Vdf)] {
        match self {
            Vdf::Block(entries) => entries,
            Vdf::Text(_) => &[],
        }
    }
}

/// The file's top block: `"AppState" { … }` gives a block with one entry.
pub fn parse(text: &str) -> Result<Vdf> {
    let mut tokens = Tokens { chars: text.trim_start_matches('\u{feff}').chars().peekable() };
    let entries = block(&mut tokens, false)?;
    Ok(Vdf::Block(entries))
}

enum Token {
    Text(String),
    Open,
    Close,
}

struct Tokens<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl Tokens<'_> {
    fn next(&mut self) -> Result<Option<Token>> {
        loop {
            match self.chars.peek() {
                None => return Ok(None),
                Some(c) if c.is_whitespace() => {
                    self.chars.next();
                }
                Some('/') => {
                    self.chars.next();
                    if self.chars.next_if_eq(&'/').is_none() {
                        bail!("barra solta no VDF");
                    }
                    while self.chars.next().is_some_and(|c| c != '\n') {}
                }
                Some('{') => {
                    self.chars.next();
                    return Ok(Some(Token::Open));
                }
                Some('}') => {
                    self.chars.next();
                    return Ok(Some(Token::Close));
                }
                Some('"') => {
                    self.chars.next();
                    return self.quoted().map(|text| Some(Token::Text(text)));
                }
                // Unquoted words happen in older files.
                Some(_) => {
                    let mut text = String::new();
                    while let Some(c) = self.chars.next_if(|c| !c.is_whitespace() && !matches!(c, '{' | '}' | '"')) {
                        text.push(c);
                    }
                    return Ok(Some(Token::Text(text)));
                }
            }
        }
    }

    fn quoted(&mut self) -> Result<String> {
        let mut text = String::new();
        loop {
            match self.chars.next() {
                None => bail!("texto sem fechar aspas no VDF"),
                Some('"') => return Ok(text),
                Some('\\') => match self.chars.next() {
                    Some('n') => text.push('\n'),
                    Some('t') => text.push('\t'),
                    Some(c) => text.push(c),
                    None => bail!("texto sem fechar aspas no VDF"),
                },
                Some(c) => text.push(c),
            }
        }
    }
}

/// Key and value pairs up to a `}` (`nested`) or the end of the file.
fn block(tokens: &mut Tokens, nested: bool) -> Result<Vec<(String, Vdf)>> {
    let mut entries = Vec::new();
    loop {
        let key = match tokens.next()? {
            None if nested => bail!("bloco sem fechar no VDF"),
            None => return Ok(entries),
            Some(Token::Close) if nested => return Ok(entries),
            Some(Token::Close) => bail!("}} sobrando no VDF"),
            Some(Token::Open) => bail!("{{ sem chave no VDF"),
            Some(Token::Text(key)) => key,
        };
        let value = match tokens.next()? {
            Some(Token::Text(text)) => Vdf::Text(text),
            Some(Token::Open) => Vdf::Block(block(tokens, true)?),
            _ => bail!("a chave {key:?} ficou sem valor no VDF"),
        };
        entries.push((key, value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_steam_files() {
        let text = "\u{feff}\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t\t// a comment\n\t\t\"apps\"\n\t\t{\n\t\t\t\"228980\"\t\t\"132143724\"\n\t\t}\n\t}\n}\n";
        let vdf = parse(text).unwrap();
        let folders = vdf.get("LibraryFolders").unwrap();
        let first = folders.get("0").unwrap();
        assert_eq!(first.text("path"), Some(r"C:\Program Files (x86)\Steam"));
        assert_eq!(first.get("apps").unwrap().entries()[0].0, "228980");
        assert_eq!(first.text("apps"), None, "a block is no text");
        assert_eq!(folders.entries().len(), 1);
    }

    #[test]
    fn unquoted_words_and_escapes() {
        let vdf = parse(r#"AppState { name "Jogo \"Bom\"\tII" appid 10 }"#).unwrap();
        let state = vdf.get("appstate").unwrap();
        assert_eq!(state.text("name"), Some("Jogo \"Bom\"\tII"));
        assert_eq!(state.text("appid"), Some("10"));
    }

    #[test]
    fn broken_files_are_errors() {
        for bad in [r#""a" { "b" "c""#, r#""a" "b" }"#, r#""a""#, r#""a "#, r#"{ "a" "b" }"#, "\"a\" / \"b\""] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        assert_eq!(parse("").unwrap(), Vdf::Block(Vec::new()));
    }
}
