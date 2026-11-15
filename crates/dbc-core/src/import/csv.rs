//! An RFC 4180 reader that keeps whether each field was quoted, so `""` (empty text) and an
//! empty field (`NULL` by default) stay apart, as the CSV export writes them (D-034).

use std::io::BufRead;

use super::ImportError;

/// One field: its text, and whether it was written in quotes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub text: String,
    pub quoted: bool,
}

/// One record and the line it starts on (1-based).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub line: u64,
    pub fields: Vec<Field>,
}

/// The records of a CSV file. Blank lines are skipped; a UTF-8 byte order mark is dropped.
pub struct CsvRecords<R> {
    reader: R,
    delimiter: char,
    /// Physical lines read so far.
    line: u64,
    failed: bool,
}

impl<R: BufRead> CsvRecords<R> {
    pub fn new(reader: R, delimiter: char) -> Self {
        Self {
            reader,
            delimiter,
            line: 0,
            failed: false,
        }
    }

    /// The next physical line with its line break, or `None` at the end of the file.
    fn read_line(&mut self) -> Result<Option<String>, ImportError> {
        let mut bytes = Vec::new();
        let read = self
            .reader
            .read_until(b'\n', &mut bytes)
            .map_err(|e| ImportError::Io(format!("The file could not be read: {e}")))?;
        if read == 0 {
            return Ok(None);
        }
        self.line += 1;
        let text = String::from_utf8(bytes).map_err(|_| ImportError::Csv {
            line: self.line,
            message: "the file is not UTF-8 text".into(),
        })?;
        Ok(Some(if self.line == 1 {
            text.trim_start_matches('\u{feff}').to_owned()
        } else {
            text
        }))
    }

    fn next_record(&mut self) -> Result<Option<Record>, ImportError> {
        let (start, mut text) = loop {
            let Some(text) = self.read_line()? else {
                return Ok(None);
            };
            if !text.trim_end_matches(['\r', '\n']).is_empty() {
                break (self.line, text);
            }
        };
        // An odd number of quotes means a quoted field holds a line break: read on.
        while !text.matches('"').count().is_multiple_of(2) {
            match self.read_line()? {
                Some(more) => text.push_str(&more),
                None => break,
            }
        }
        let body = text
            .strip_suffix('\n')
            .map(|t| t.strip_suffix('\r').unwrap_or(t))
            .unwrap_or(&text);
        let fields = parse(body, self.delimiter).map_err(|message| ImportError::Csv {
            line: start,
            message: message.into(),
        })?;
        Ok(Some(Record {
            line: start,
            fields,
        }))
    }
}

impl<R: BufRead> Iterator for CsvRecords<R> {
    type Item = Result<Record, ImportError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        let next = self.next_record().transpose();
        self.failed = matches!(next, Some(Err(_)));
        next
    }
}

enum State {
    Start,
    Unquoted,
    Quoted,
    /// A quote inside a quoted field: the end of the field, or the first of `""`.
    QuoteInQuoted,
}

/// The fields of one record, line breaks at its end removed.
fn parse(text: &str, delimiter: char) -> Result<Vec<Field>, &'static str> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut state = State::Start;
    let mut push = |current: &mut String, quoted: &mut bool| {
        fields.push(Field {
            text: std::mem::take(current),
            quoted: std::mem::take(quoted),
        });
    };
    for c in text.chars() {
        state = match state {
            State::Start if c == '"' => {
                quoted = true;
                State::Quoted
            }
            State::Start | State::Unquoted if c == delimiter => {
                push(&mut current, &mut quoted);
                State::Start
            }
            State::Start | State::Unquoted if c == '"' => {
                return Err(
                    "a quote inside an unquoted field; quote the field and double the quote",
                );
            }
            State::Start | State::Unquoted => {
                current.push(c);
                State::Unquoted
            }
            State::Quoted if c == '"' => State::QuoteInQuoted,
            State::Quoted => {
                current.push(c);
                State::Quoted
            }
            State::QuoteInQuoted if c == '"' => {
                current.push('"');
                State::Quoted
            }
            State::QuoteInQuoted if c == delimiter => {
                push(&mut current, &mut quoted);
                State::Start
            }
            State::QuoteInQuoted => return Err("text after a closing quote"),
        };
    }
    if matches!(state, State::Quoted) {
        return Err("a quoted field is not closed");
    }
    push(&mut current, &mut quoted);
    Ok(fields)
}
