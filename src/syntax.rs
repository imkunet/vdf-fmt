use anyhow::{Result, bail};

const LINE_COMMENT: &str = "//";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LineToken {
    Word(String),
    OpenBrace,
    CloseBrace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedLine {
    pub(crate) tokens: Vec<LineToken>,
    pub(crate) trailing_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DisabledPair {
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) trailing_comment: Option<String>,
}

pub(crate) fn normalize_newlines(input: &str) -> String {
    input.replace("\r\n", "\n").replace('\r', "\n")
}

pub(crate) fn parse_line(line: &str) -> Result<ParsedLine> {
    let mut tokens = Vec::new();
    let mut chars = line.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        match ch {
            ch if ch.is_whitespace() => {}
            '/' if line[index..].starts_with(LINE_COMMENT) => {
                return Ok(ParsedLine {
                    tokens,
                    trailing_comment: Some(line[index..].trim().to_owned()),
                });
            }
            '"' => tokens.push(LineToken::Word(read_quoted_token(line, index, &mut chars)?)),
            '{' => tokens.push(LineToken::OpenBrace),
            '}' => tokens.push(LineToken::CloseBrace),
            _ => tokens.push(LineToken::Word(read_bare_token(line, index, &mut chars))),
        }
    }

    Ok(ParsedLine {
        tokens,
        trailing_comment: None,
    })
}

pub(crate) fn normalize_key(token: &str) -> String {
    let key_without_quotes = match without_surrounding_quotes(token) {
        Some(key_without_quotes) => key_without_quotes,
        None => return token.to_owned(),
    };

    if can_be_bare_token(key_without_quotes) {
        key_without_quotes.to_owned()
    } else {
        token.to_owned()
    }
}

pub(crate) fn normalize_value_tokens(tokens: &[String], bare_literals: bool) -> String {
    let value = tokens.join(" ");
    if tokens.len() == 1 {
        normalize_one_value_token(&value, bare_literals)
    } else {
        quote_value(&value)
    }
}

pub(crate) fn parse_disabled_pair(comment: &str, bare_literals: bool) -> Option<DisabledPair> {
    let body = disabled_pair_body(comment)?;
    let parsed = parse_line(body).ok()?;
    let (key, value) = two_word_tokens(&parsed.tokens)?;

    if !is_disabled_pair_key(key) || !is_disabled_pair_value(value) {
        return None;
    }

    Some(DisabledPair {
        key: normalize_key(key),
        value: normalize_one_value_token(value, bare_literals),
        trailing_comment: parsed.trailing_comment,
    })
}

fn read_quoted_token(
    line: &str,
    start: usize,
    chars: &mut impl Iterator<Item = (usize, char)>,
) -> Result<String> {
    let mut escaped = false;

    for (index, ch) in chars {
        match (escaped, ch) {
            (true, _) => escaped = false,
            (false, '\\') => escaped = true,
            (false, '"') => return Ok(line[start..index + ch.len_utf8()].to_owned()),
            _ => {}
        }
    }

    bail!("unterminated quoted token");
}

fn read_bare_token(
    line: &str,
    start: usize,
    chars: &mut std::iter::Peekable<impl Iterator<Item = (usize, char)>>,
) -> String {
    let mut end = line.len();

    while let Some(&(index, ch)) = chars.peek() {
        if ends_bare_token(line, index, ch) {
            end = index;
            break;
        }
        chars.next();
    }

    line[start..end].to_owned()
}

fn ends_bare_token(line: &str, index: usize, ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '{' | '}' | '"') || line[index..].starts_with(LINE_COMMENT)
}

fn disabled_pair_body(comment: &str) -> Option<&str> {
    let comment_text = comment.strip_prefix(LINE_COMMENT)?;
    Some(comment_text.strip_prefix(' ').unwrap_or(comment_text))
}

fn two_word_tokens(tokens: &[LineToken]) -> Option<(&str, &str)> {
    match tokens {
        [LineToken::Word(first), LineToken::Word(second)] => Some((first, second)),
        _ => None,
    }
}

fn is_disabled_pair_key(token: &str) -> bool {
    let token = without_surrounding_quotes(token).unwrap_or(token);

    let mut chars = token.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':' | '-'))
}

fn is_disabled_pair_value(token: &str) -> bool {
    is_quoted_token(token) || is_bare_value(token)
}

fn normalize_one_value_token(token: &str, bare_literals: bool) -> String {
    if let Some(unquoted) = without_surrounding_quotes(token) {
        if bare_literals && is_bare_value(unquoted) {
            unquoted.to_owned()
        } else {
            token.to_owned()
        }
    } else if bare_literals && is_bare_value(token) {
        token.to_owned()
    } else {
        quote_value(token)
    }
}

fn is_quoted_token(token: &str) -> bool {
    without_surrounding_quotes(token).is_some()
}

fn without_surrounding_quotes(token: &str) -> Option<&str> {
    let token = token.strip_prefix('"')?;
    token.strip_suffix('"')
}

fn is_bare_value(value: &str) -> bool {
    matches!(value, "true" | "false") || is_number(value)
}

fn is_number(value: &str) -> bool {
    let value = value.strip_prefix('-').unwrap_or(value);

    if value.is_empty() {
        return false;
    }

    match value.split_once('.') {
        Some((whole, fraction)) => {
            (!whole.is_empty() || !fraction.is_empty())
                && whole.chars().all(|ch| ch.is_ascii_digit())
                && fraction.chars().all(|ch| ch.is_ascii_digit())
        }
        None => value.chars().all(|ch| ch.is_ascii_digit()),
    }
}

fn quote_value(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for ch in value.chars() {
        if matches!(ch, '\\' | '"') {
            quoted.push('\\');
        }
        quoted.push(ch);
    }
    quoted.push('"');
    quoted
}

fn can_be_bare_token(token: &str) -> bool {
    !token.is_empty()
        && !token.contains("//")
        && token
            .chars()
            .all(|ch| !ch.is_whitespace() && !ch.is_control() && !matches!(ch, '"' | '{' | '}'))
}
