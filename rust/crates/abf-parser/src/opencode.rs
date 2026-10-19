use crate::{Code, Diagnostic, Diagnostics, Field, Limits, Observation, ParseResult, Status};

// Private, bounded JSON tree. Arrays are validated then discarded: never an extracted field.
// Number spelling is borrowed so integer precision/range is never lost through floating point.
#[derive(Debug)]
enum Node<'a> {
    Object(Vec<(String, Node<'a>)>),
    String(String),
    Number(&'a str),
    Null,
    Other,
}
impl<'a> Node<'a> {
    fn get(&self, key: &str) -> Option<&Node<'a>> {
        match self {
            Self::Object(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

struct Decoder<'a> {
    text: &'a str,
    pos: usize,
    entries: usize,
    limits: Limits,
}
impl<'a> Decoder<'a> {
    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }
    fn bump(&mut self) -> Result<u8, Code> {
        let byte = self.peek().ok_or(Code::IncompleteJson)?;
        self.pos = self.pos.checked_add(1).ok_or(Code::FrameBudget)?;
        Ok(byte)
    }
    fn whitespace(&mut self) -> Result<(), Code> {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.bump()?;
        }
        Ok(())
    }
    fn expect(&mut self, byte: u8) -> Result<(), Code> {
        if self.bump()? == byte {
            Ok(())
        } else {
            Err(Code::MalformedJson)
        }
    }
    fn entry(&mut self) -> Result<(), Code> {
        self.entries = self.entries.checked_add(1).ok_or(Code::EntryBudget)?;
        if self.entries > self.limits.entries {
            Err(Code::EntryBudget)
        } else {
            Ok(())
        }
    }
    fn hex4(&mut self) -> Result<u32, Code> {
        let mut value = 0u32;
        for _ in 0..4 {
            let digit = match self.bump()? {
                c @ b'0'..=b'9' => u32::from(c - b'0'),
                c @ b'a'..=b'f' => u32::from(c - b'a') + 10,
                c @ b'A'..=b'F' => u32::from(c - b'A') + 10,
                _ => return Err(Code::MalformedJson),
            };
            value = value
                .checked_mul(16)
                .and_then(|v| v.checked_add(digit))
                .ok_or(Code::MalformedJson)?;
        }
        Ok(value)
    }
    fn string(&mut self) -> Result<String, Code> {
        self.expect(b'"')?;
        // Scan first to allocate exactly the encoded interior length (an upper bound
        // on decoded UTF-8). All string interiors occupy disjoint input spans.
        let start = self.pos;
        let mut end = start;
        let mut escaped = false;
        loop {
            let c = *self.text.as_bytes().get(end).ok_or(Code::IncompleteJson)?;
            if c == b'"' && !escaped {
                break;
            }
            escaped = c == b'\\' && !escaped;
            end = end.checked_add(1).ok_or(Code::FrameBudget)?;
        }
        let mut out = String::with_capacity(end.checked_sub(start).ok_or(Code::MalformedJson)?);
        while self.pos < end {
            match self.bump()? {
                0..=31 => return Err(Code::MalformedJson),
                b'\\' => {
                    let c = match self.bump()? {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let first = self.hex4()?;
                            let scalar = if (0xD800..=0xDBFF).contains(&first) {
                                self.expect(b'\\')?;
                                self.expect(b'u')?;
                                let second = self.hex4()?;
                                if !(0xDC00..=0xDFFF).contains(&second) {
                                    return Err(Code::MalformedJson);
                                }
                                first
                                    .checked_sub(0xD800)
                                    .and_then(|n| n.checked_mul(1024))
                                    .and_then(|n| n.checked_add(second - 0xDC00))
                                    .and_then(|n| n.checked_add(0x10000))
                                    .ok_or(Code::MalformedJson)?
                            } else {
                                first
                            };
                            char::from_u32(scalar).ok_or(Code::MalformedJson)?
                        }
                        _ => return Err(Code::MalformedJson),
                    };
                    if self.pos > end {
                        return Err(Code::MalformedJson);
                    }
                    out.push(c);
                }
                byte if byte < 128 => out.push(char::from(byte)),
                _ => {
                    let previous = self.pos.checked_sub(1).ok_or(Code::MalformedJson)?;
                    let c = self.text[previous..]
                        .chars()
                        .next()
                        .ok_or(Code::IncompleteJson)?;
                    self.pos = previous
                        .checked_add(c.len_utf8())
                        .ok_or(Code::FrameBudget)?;
                    out.push(c);
                }
            }
            if self.pos > end {
                return Err(Code::MalformedJson);
            }
        }
        self.expect(b'"')?;
        Ok(out)
    }
    fn digits(&mut self) -> Result<(), Code> {
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.bump()?;
        }
        if self.pos == start {
            Err(if self.peek().is_none() {
                Code::IncompleteJson
            } else {
                Code::MalformedJson
            })
        } else {
            Ok(())
        }
    }
    fn number(&mut self) -> Result<Node<'a>, Code> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.bump()?;
        }
        match self.peek() {
            Some(b'0') => {
                self.bump()?;
            }
            Some(b'1'..=b'9') => self.digits()?,
            None => return Err(Code::IncompleteJson),
            _ => return Err(Code::MalformedJson),
        }
        if self.peek() == Some(b'.') {
            self.bump()?;
            self.digits()?;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.bump()?;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.bump()?;
            }
            self.digits()?;
        }
        Ok(Node::Number(&self.text[start..self.pos]))
    }
    fn value(&mut self, depth: usize) -> Result<Node<'a>, Code> {
        self.whitespace()?;
        match self.peek().ok_or(Code::IncompleteJson)? {
            b'{' | b'[' => {
                let depth = depth.checked_add(1).ok_or(Code::DepthBudget)?;
                if depth > self.limits.depth {
                    return Err(Code::DepthBudget);
                }
                let object = self.bump()? == b'{';
                let close = if object { b'}' } else { b']' };
                let mut members: Vec<(String, Node<'a>)> = Vec::new();
                self.whitespace()?;
                if self.peek() != Some(close) {
                    loop {
                        self.entry()?;
                        self.whitespace()?;
                        if object {
                            let key = self.string()?;
                            if members.iter().any(|(k, _)| *k == key) {
                                return Err(Code::DuplicateKey);
                            }
                            self.whitespace()?;
                            self.expect(b':')?;
                            let node = self.value(depth)?;
                            // Exact growth: requested pair storage is bounded by counted entries.
                            members.reserve_exact(1);
                            members.push((key, node));
                        } else {
                            self.value(depth)?;
                        }
                        self.whitespace()?;
                        match self.bump()? {
                            c if c == close => break,
                            b',' => continue,
                            _ => return Err(Code::MalformedJson),
                        }
                    }
                } else {
                    self.bump()?;
                }
                Ok(if object {
                    Node::Object(members)
                } else {
                    Node::Other
                })
            }
            b'"' => self.string().map(Node::String),
            b'-' | b'0'..=b'9' => self.number(),
            b'n' | b't' | b'f' => {
                let word: &[u8] = match self.peek() {
                    Some(b'n') => b"null",
                    Some(b't') => b"true",
                    _ => b"false",
                };
                for byte in word {
                    self.expect(*byte)?;
                }
                Ok(if word == b"null" {
                    Node::Null
                } else {
                    Node::Other
                })
            }
            _ => Err(Code::MalformedJson),
        }
    }
}

fn container(node: Option<&Node<'_>>) -> Field<()> {
    match node {
        None => Field::Absent,
        Some(Node::Null) => Field::Null,
        Some(Node::Object(_)) => Field::Valid(()),
        _ => Field::WrongType,
    }
}
fn metadata(node: Option<&Node<'_>>, limits: Limits) -> Result<Field<Box<str>>, Code> {
    Ok(match node {
        None => Field::Absent,
        Some(Node::Null) => Field::Null,
        Some(Node::String(s)) => {
            if s.len() > limits.string_bytes {
                return Err(Code::StringBudget);
            }
            Field::Valid(s.as_str().into())
        }
        _ => Field::WrongType,
    })
}
fn integer<T: std::str::FromStr>(node: Option<&Node<'_>>) -> Field<T> {
    match node {
        None => Field::Absent,
        Some(Node::Null) => Field::Null,
        Some(Node::Number(s)) if !s.contains(['.', 'e', 'E']) => match s.parse::<T>() {
            Ok(n) => Field::Valid(n),
            Err(_) => Field::OutOfRange,
        },
        _ => Field::WrongType,
    }
}
fn child_integer<T: std::str::FromStr>(parent: Option<&Node<'_>>, key: &str) -> Field<T> {
    match parent {
        Some(node @ Node::Object(_)) => integer(node.get(key)),
        _ => Field::Unsupported,
    }
}
fn diagnose<T>(field: &Field<T>, diagnostics: &mut Diagnostics) {
    match field {
        Field::WrongType | Field::OutOfRange => diagnostics.insert(Diagnostic::InvalidField),
        Field::Null => diagnostics.insert(Diagnostic::ExplicitNull),
        _ => {}
    }
}
fn unknown_components(node: Option<&Node<'_>>, allowed: &[&str]) -> bool {
    match node {
        Some(Node::Object(members)) => members
            .iter()
            .any(|(key, _)| !allowed.contains(&key.as_str())),
        _ => false,
    }
}
fn observe(
    root: &Node<'_>,
    correlation: Option<&str>,
    limits: Limits,
) -> Result<Observation, Code> {
    let tokens = root.get("tokens");
    let cache = tokens.and_then(|t| t.get("cache"));
    let time = root.get("time");
    let mut o = Observation {
        correlation: correlation.map(Box::from),
        message_id: metadata(root.get("id"), limits)?,
        session_id: metadata(root.get("sessionID"), limits)?,
        model_id: metadata(root.get("modelID"), limits)?,
        provider_id: metadata(root.get("providerID"), limits)?,
        time: container(time),
        created: child_integer(time, "created"),
        tokens: container(tokens),
        cache: if matches!(tokens, Some(Node::Object(_))) {
            container(cache)
        } else {
            Field::Unsupported
        },
        input: child_integer(tokens, "input"),
        output: child_integer(tokens, "output"),
        cache_read: child_integer(cache, "read"),
        cache_write: child_integer(cache, "write"),
        total: child_integer(tokens, "total"),
        status: Status::Accepted,
        diagnostics: Diagnostics::default(),
    };
    for field in [&o.message_id, &o.session_id, &o.model_id, &o.provider_id] {
        diagnose(field, &mut o.diagnostics);
    }
    for field in [&o.tokens, &o.cache, &o.time] {
        diagnose(field, &mut o.diagnostics);
    }
    diagnose(&o.created, &mut o.diagnostics);
    for field in [&o.input, &o.output, &o.cache_read, &o.cache_write, &o.total] {
        diagnose(field, &mut o.diagnostics);
    }
    if unknown_components(tokens, &["input", "output", "total", "cache"])
        || unknown_components(cache, &["read", "write"])
    {
        o.diagnostics.insert(Diagnostic::UnsupportedComponent);
    }
    let any_valid = [&o.input, &o.output, &o.cache_read, &o.cache_write, &o.total]
        .iter()
        .any(|f| matches!(f, Field::Valid(_)));
    if !any_valid {
        o.diagnostics.insert(Diagnostic::IncompleteTokens);
        o.status = Status::Incomplete;
    } else if o.diagnostics.bits() != 0 {
        o.status = Status::Qualified;
    }
    Ok(o)
}

/// Parse one complete caller-owned synthetic frame. No discovery, identity or sequence logic.
pub fn parse(frame: &[u8], correlation: Option<&str>, limits: Limits) -> ParseResult {
    fn inner(frame: &[u8], correlation: Option<&str>, limits: Limits) -> Result<ParseResult, Code> {
        if !limits.valid() {
            return Err(Code::InvalidLimits);
        }
        if frame.len() > limits.frame_bytes {
            return Err(Code::FrameBudget);
        }
        if correlation.is_some_and(|s| s.len() > limits.string_bytes) {
            return Err(Code::StringBudget);
        }
        let text = std::str::from_utf8(frame).map_err(|_| Code::InvalidUtf8)?;
        let mut decoder = Decoder {
            text,
            pos: 0,
            entries: 0,
            limits,
        };
        let root = decoder.value(0)?;
        decoder.whitespace()?;
        if decoder.peek().is_some() {
            return Err(Code::TrailingData);
        }
        if !matches!(root, Node::Object(_)) {
            return Err(Code::RootShape);
        }
        let result = ParseResult::Observation(observe(&root, correlation, limits)?);
        if result.retained_bytes().ok_or(Code::ResultBudget)? > limits.result_bytes {
            return Err(Code::ResultBudget);
        }
        Ok(result)
    }
    inner(frame, correlation, limits).unwrap_or_else(ParseResult::Rejected)
}
