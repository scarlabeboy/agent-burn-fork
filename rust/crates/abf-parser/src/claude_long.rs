use crate::claude::{
    ClaudeObservation, ClaudeParseResult, KNOWN_BREAKDOWN_KEYS, KNOWN_USAGE_KEYS, finalize,
};
use crate::{Code, Diagnostics, Field, Status};

/// Ceilings for [`parse_claude_long`]. Callers may lower, but never raise, each value.
///
/// `entries` counts every object member and array element in the record, retained or
/// skipped. Together with `record_bytes` it bounds processing work; `depth` bounds the
/// recursion; `string_bytes` bounds every retained string; `result_bytes` bounds the result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LongLimits {
    pub record_bytes: usize,
    pub depth: usize,
    pub entries: usize,
    pub string_bytes: usize,
    pub result_bytes: usize,
}
impl LongLimits {
    pub const MAX: Self = Self {
        record_bytes: 8 * 1024 * 1024,
        depth: 32,
        entries: 1 << 20,
        string_bytes: 256,
        result_bytes: 4096,
    };
    fn valid(self) -> bool {
        self.record_bytes <= Self::MAX.record_bytes
            && self.depth <= Self::MAX.depth
            && self.entries <= Self::MAX.entries
            && self.string_bytes <= Self::MAX.string_bytes
            && self.result_bytes <= Self::MAX.result_bytes
            && self.result_bytes >= size_of::<ClaudeParseResult>()
    }
}

// Decoded-length cap for object keys on the path to a retained claim. A longer key cannot
// equal an allowlisted name and is treated as an unmatched key, never as an error.
const KEY_CAP: usize = 64;

enum Str {
    Skipped,
    Kept(String),
    TooLong,
}
enum Kind {
    Null,
    Wrong,
    Object,
}

fn once(seen: &mut bool) -> Result<(), Code> {
    if std::mem::replace(seen, true) {
        Err(Code::DuplicateKey)
    } else {
        Ok(())
    }
}

// Single pass over borrowed, already UTF-8-validated text. No tree is built: skipped values
// are validated and dropped as they are read, so heap use is independent of the record size.
struct Scan<'a> {
    text: &'a str,
    pos: usize,
    entries: usize,
    limits: LongLimits,
}
impl<'a> Scan<'a> {
    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }
    fn bump(&mut self) -> Result<u8, Code> {
        let byte = self.peek().ok_or(Code::IncompleteJson)?;
        self.pos = self.pos.checked_add(1).ok_or(Code::FrameBudget)?;
        Ok(byte)
    }
    fn ws(&mut self) -> Result<(), Code> {
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
    fn enter(&self, depth: usize) -> Result<usize, Code> {
        let depth = depth.checked_add(1).ok_or(Code::DepthBudget)?;
        if depth > self.limits.depth {
            Err(Code::DepthBudget)
        } else {
            Ok(depth)
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
            value = value * 16 + digit;
        }
        Ok(value)
    }
    // After a backslash: same escape grammar as the tree decoder, surrogate pairs included.
    fn escape(&mut self) -> Result<char, Code> {
        Ok(match self.bump()? {
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
                    0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)
                } else {
                    first
                };
                char::from_u32(scalar).ok_or(Code::MalformedJson)?
            }
            _ => return Err(Code::MalformedJson),
        })
    }
    /// Validate one string. With `cap`, decode at most `cap` bytes (no up-front allocation
    /// sized from the input); a longer string is still fully validated, then `TooLong`.
    fn string(&mut self, cap: Option<usize>) -> Result<Str, Code> {
        self.expect(b'"')?;
        let mut out = cap.map(|_| String::new());
        let mut over = false;
        loop {
            let ch = match self.bump()? {
                b'"' => break,
                0..=31 => return Err(Code::MalformedJson),
                b'\\' => self.escape()?,
                byte if byte < 128 => char::from(byte),
                _ => {
                    let previous = self.pos - 1;
                    let ch = self.text[previous..]
                        .chars()
                        .next()
                        .ok_or(Code::IncompleteJson)?;
                    self.pos = previous + ch.len_utf8();
                    ch
                }
            };
            if let (Some(buffer), Some(cap)) = (out.as_mut(), cap) {
                if buffer.len() + ch.len_utf8() > cap {
                    over = true;
                } else if !over {
                    buffer.push(ch);
                }
            }
        }
        Ok(match out {
            None => Str::Skipped,
            Some(_) if over => Str::TooLong,
            Some(buffer) => Str::Kept(buffer),
        })
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
    fn number(&mut self) -> Result<&'a str, Code> {
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
        Ok(&self.text[start..self.pos])
    }
    fn literal(&mut self, word: &[u8]) -> Result<(), Code> {
        for byte in word {
            self.expect(*byte)?;
        }
        Ok(())
    }
    /// Validate and drop one value. Skipped objects are not checked for duplicate keys:
    /// that needs memory proportional to their member count.
    fn skip_value(&mut self, depth: usize) -> Result<(), Code> {
        self.ws()?;
        match self.peek().ok_or(Code::IncompleteJson)? {
            open @ (b'{' | b'[') => {
                let depth = self.enter(depth)?;
                let object = open == b'{';
                let close = if object { b'}' } else { b']' };
                self.bump()?;
                self.ws()?;
                if self.peek() == Some(close) {
                    self.bump()?;
                    return Ok(());
                }
                loop {
                    self.entry()?;
                    self.ws()?;
                    if object {
                        self.string(None)?;
                        self.ws()?;
                        self.expect(b':')?;
                    }
                    self.skip_value(depth)?;
                    self.ws()?;
                    match self.bump()? {
                        c if c == close => return Ok(()),
                        b',' => {}
                        _ => return Err(Code::MalformedJson),
                    }
                }
            }
            b'"' => self.string(None).map(drop),
            b'-' | b'0'..=b'9' => self.number().map(drop),
            b'n' => self.literal(b"null"),
            b't' => self.literal(b"true"),
            b'f' => self.literal(b"false"),
            _ => Err(Code::MalformedJson),
        }
    }
    /// Walk the members of the object at the cursor (`depth` is the object's own depth).
    /// `visit` must consume exactly one value for every key it is given.
    fn members<F>(&mut self, depth: usize, mut visit: F) -> Result<(), Code>
    where
        F: FnMut(&mut Self, Option<String>, usize) -> Result<(), Code>,
    {
        self.expect(b'{')?;
        self.ws()?;
        if self.peek() == Some(b'}') {
            return self.bump().map(drop);
        }
        loop {
            self.entry()?;
            self.ws()?;
            let key = match self.string(Some(KEY_CAP))? {
                Str::Kept(key) => Some(key),
                _ => None,
            };
            self.ws()?;
            self.expect(b':')?;
            visit(self, key, depth)?;
            self.ws()?;
            match self.bump()? {
                b'}' => return Ok(()),
                b',' => {}
                _ => return Err(Code::MalformedJson),
            }
        }
    }
    fn meta(&mut self, depth: usize) -> Result<Field<Box<str>>, Code> {
        self.ws()?;
        match self.peek().ok_or(Code::IncompleteJson)? {
            b'"' => match self.string(Some(self.limits.string_bytes))? {
                Str::Kept(value) => Ok(Field::Valid(value.into())),
                _ => Err(Code::StringBudget),
            },
            b'n' => self.literal(b"null").map(|()| Field::Null),
            _ => self.skip_value(depth).map(|()| Field::WrongType),
        }
    }
    fn int(&mut self, depth: usize) -> Result<Field<u64>, Code> {
        self.ws()?;
        match self.peek().ok_or(Code::IncompleteJson)? {
            b'-' | b'0'..=b'9' => {
                let lexeme = self.number()?;
                Ok(if lexeme.contains(['.', 'e', 'E']) {
                    Field::WrongType
                } else {
                    lexeme.parse().map_or(Field::OutOfRange, Field::Valid)
                })
            }
            b'n' => self.literal(b"null").map(|()| Field::Null),
            _ => self.skip_value(depth).map(|()| Field::WrongType),
        }
    }
    // Does not consume an object; null and other values are consumed.
    fn kind(&mut self, depth: usize) -> Result<Kind, Code> {
        self.ws()?;
        match self.peek().ok_or(Code::IncompleteJson)? {
            b'{' => Ok(Kind::Object),
            b'n' => self.literal(b"null").map(|()| Kind::Null),
            _ => self.skip_value(depth).map(|()| Kind::Wrong),
        }
    }

    fn root(&mut self, o: &mut ClaudeObservation, unsupported: &mut bool) -> Result<(), Code> {
        let depth = self.enter(0)?;
        let (mut session, mut request, mut timestamp, mut message) = (false, false, false, false);
        self.members(depth, |s, key, depth| match key.as_deref() {
            Some("sessionId") => {
                once(&mut session)?;
                o.session_id = s.meta(depth)?;
                Ok(())
            }
            Some("requestId") => {
                once(&mut request)?;
                o.request_id = s.meta(depth)?;
                Ok(())
            }
            Some("timestamp") => {
                once(&mut timestamp)?;
                o.timestamp = s.meta(depth)?;
                Ok(())
            }
            Some("message") => {
                once(&mut message)?;
                s.message(depth, o, unsupported)
            }
            _ => s.skip_value(depth),
        })
    }
    fn message(
        &mut self,
        depth: usize,
        o: &mut ClaudeObservation,
        unsupported: &mut bool,
    ) -> Result<(), Code> {
        match self.kind(depth)? {
            Kind::Null => o.message = Field::Null,
            Kind::Wrong => o.message = Field::WrongType,
            Kind::Object => {
                o.message = Field::Valid(());
                (o.message_id, o.model, o.usage) = (Field::Absent, Field::Absent, Field::Absent);
                let depth = self.enter(depth)?;
                let (mut id, mut model, mut usage) = (false, false, false);
                self.members(depth, |s, key, depth| match key.as_deref() {
                    Some("id") => {
                        once(&mut id)?;
                        o.message_id = s.meta(depth)?;
                        Ok(())
                    }
                    Some("model") => {
                        once(&mut model)?;
                        o.model = s.meta(depth)?;
                        Ok(())
                    }
                    Some("usage") => {
                        once(&mut usage)?;
                        s.usage(depth, o, unsupported)
                    }
                    _ => s.skip_value(depth),
                })?;
            }
        }
        Ok(())
    }
    fn usage(
        &mut self,
        depth: usize,
        o: &mut ClaudeObservation,
        unsupported: &mut bool,
    ) -> Result<(), Code> {
        match self.kind(depth)? {
            Kind::Null => o.usage = Field::Null,
            Kind::Wrong => o.usage = Field::WrongType,
            Kind::Object => {
                o.usage = Field::Valid(());
                o.input = Field::Absent;
                o.output = Field::Absent;
                o.cache_read = Field::Absent;
                o.cache_creation_flat = Field::Absent;
                o.cache_creation_detail = Field::Absent;
                let depth = self.enter(depth)?;
                let mut seen = [false; 5];
                self.members(depth, |s, key, depth| {
                    let name = key.as_deref();
                    let slot = match name {
                        Some("input_tokens") => 0,
                        Some("output_tokens") => 1,
                        Some("cache_read_input_tokens") => 2,
                        Some("cache_creation_input_tokens") => 3,
                        Some("cache_creation") => 4,
                        _ => {
                            // Unknown token-like keys may hide uncounted components. An
                            // over-long key cannot be classified and is treated the same.
                            *unsupported |= name.is_none_or(|n| {
                                n.ends_with("_tokens") && !KNOWN_USAGE_KEYS.contains(&n)
                            });
                            return s.skip_value(depth);
                        }
                    };
                    once(&mut seen[slot])?;
                    match slot {
                        0 => o.input = s.int(depth)?,
                        1 => o.output = s.int(depth)?,
                        2 => o.cache_read = s.int(depth)?,
                        3 => o.cache_creation_flat = s.int(depth)?,
                        _ => return s.detail(depth, o, unsupported),
                    }
                    Ok(())
                })?;
            }
        }
        Ok(())
    }
    fn detail(
        &mut self,
        depth: usize,
        o: &mut ClaudeObservation,
        unsupported: &mut bool,
    ) -> Result<(), Code> {
        match self.kind(depth)? {
            Kind::Null => o.cache_creation_detail = Field::Null,
            Kind::Wrong => o.cache_creation_detail = Field::WrongType,
            Kind::Object => {
                o.cache_creation_detail = Field::Valid(());
                o.cache_creation_5m = Field::Absent;
                o.cache_creation_1h = Field::Absent;
                let depth = self.enter(depth)?;
                let (mut five, mut hour) = (false, false);
                self.members(depth, |s, key, depth| match key.as_deref() {
                    Some(name) if name == KNOWN_BREAKDOWN_KEYS[0] => {
                        once(&mut five)?;
                        o.cache_creation_5m = s.int(depth)?;
                        Ok(())
                    }
                    Some(name) if name == KNOWN_BREAKDOWN_KEYS[1] => {
                        once(&mut hour)?;
                        o.cache_creation_1h = s.int(depth)?;
                        Ok(())
                    }
                    _ => {
                        *unsupported = true;
                        s.skip_value(depth)
                    }
                })?;
            }
        }
        Ok(())
    }
}

/// Parse one complete, caller-owned Claude Code JSONL record whose message content may be
/// large. Same observation contract, field states and cache-creation rules as
/// [`parse_claude`](crate::parse_claude), but without the tree: heap use is bounded by the
/// retained strings (at most `string_bytes` each) and short key candidates, never by the
/// record size or its content.
///
/// Guarantees: the whole record is checked as UTF-8 and as JSON (grammar, escapes, depth,
/// entry count); a record over `record_bytes`, `depth` or `entries` is rejected with
/// `FrameBudget`, `DepthBudget` or `EntryBudget`, never truncated or counted as zero.
/// Duplicate keys are rejected for the root, `message`, `message.usage` and
/// `usage.cache_creation` objects, i.e. wherever they could affect a retained claim.
/// Duplicate keys inside skipped values (content, tool data, unknown objects) are NOT
/// detected: that would need memory proportional to the skipped object.
pub fn parse_claude_long(
    record: &[u8],
    correlation: Option<&str>,
    limits: LongLimits,
) -> ClaudeParseResult {
    fn inner(
        record: &[u8],
        correlation: Option<&str>,
        limits: LongLimits,
    ) -> Result<ClaudeParseResult, Code> {
        if !limits.valid() {
            return Err(Code::InvalidLimits);
        }
        if record.len() > limits.record_bytes {
            return Err(Code::FrameBudget);
        }
        if correlation.is_some_and(|s| s.len() > limits.string_bytes) {
            return Err(Code::StringBudget);
        }
        let text = std::str::from_utf8(record).map_err(|_| Code::InvalidUtf8)?;
        let mut scan = Scan {
            text,
            pos: 0,
            entries: 0,
            limits,
        };
        // Parents start unavailable; each is replaced as its container is actually seen.
        let mut o = ClaudeObservation {
            correlation: correlation.map(Box::from),
            session_id: Field::Absent,
            request_id: Field::Absent,
            timestamp: Field::Absent,
            message: Field::Absent,
            message_id: Field::Unsupported,
            model: Field::Unsupported,
            usage: Field::Unsupported,
            input: Field::Unsupported,
            output: Field::Unsupported,
            cache_read: Field::Unsupported,
            cache_creation_flat: Field::Unsupported,
            cache_creation_detail: Field::Unsupported,
            cache_creation_5m: Field::Unsupported,
            cache_creation_1h: Field::Unsupported,
            status: Status::Accepted,
            diagnostics: Diagnostics::default(),
        };
        let mut unsupported = false;
        scan.ws()?;
        if scan.peek() == Some(b'{') {
            scan.root(&mut o, &mut unsupported)?;
            scan.ws()?;
            if scan.peek().is_some() {
                return Err(Code::TrailingData);
            }
        } else {
            scan.skip_value(0)?;
            scan.ws()?;
            return Err(if scan.peek().is_some() {
                Code::TrailingData
            } else {
                Code::RootShape
            });
        }
        finalize(&mut o, unsupported);
        let result = ClaudeParseResult::Observation(o);
        if result.retained_bytes().ok_or(Code::ResultBudget)? > limits.result_bytes {
            return Err(Code::ResultBudget);
        }
        Ok(result)
    }
    inner(record, correlation, limits).unwrap_or_else(ClaudeParseResult::Rejected)
}
