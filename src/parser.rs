// overview: This is a very flexible parser for languages that may or may not use indentation to structure, which have a fixed number of infix operators, with precedence. This was entirely sufficient to implement the quite sophisticated syntax of the ~~Bjork~~Kaba language.
// the tokenizer converts a string into Scrips, which are sometimes like tokens, other times richer and more structured, as they can retain indent structure. The Scrips are then transformed through term rewriting into Asts.
// keywords: fn, to, if, else, elif

use std::mem::take;

use crate::{Arena, Ref};
/*
The example input:

# wait, this function syntax doesn't work how I wish it did. How should I do a function syntax... i see no good options. I am stuck. What value is there in this if it cannot be beautiful?
  # this tantrum evolved into "how can we keep the parser minimalistic and flexible" and the answer turned out to be "it should mostly be term rewriting over a tokenizer, though the tokenization may not just be tokens, it may retain indent structure and stuff"

f = fn(a:int b:int to:int a + b)
f = fn a:int b:int to:int
    a + b


print(f(1 2))

# this is a comment
#(
  this is a multiline comment
)

# I want this to translate to Code objects that're more like let combined = Name("combined") & StructType(a:int b:int). Like, a nominal type should contain its name. We could do that by having struct() take an implicit CodeContext that contains like, the ID of the Code object it's part of and the code object where it's defined.
combined = struct(a:int b:int)

fc = fn(c:combined to:int c.a + c.b)

fc = fn c:combined to int
    c.a + c.b

ac =
    if c
        c.a + c.b
    else 0

*/

#[derive(Debug, Clone, Copy)]
pub struct Span {
    pub start: usize,
    pub length: usize,
}

impl Span {
    pub fn new(start: usize, length: usize) -> Self {
        Self { start, length }
    }

    pub fn from_range(start: usize, end: usize) -> Self {
        Self {
            start,
            length: end.saturating_sub(start),
        }
    }
    pub fn end(&self) -> usize {
        self.start + self.length
    }
}

/// A stage of parsing between raw string and AST. Notices tokens, quoted strings, parens, and indents.
#[derive(Debug)]
pub struct Tokk {
    pub span: Span,
    pub content: TokkV,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuoteType {
    Single,   // '
    Double,   // "
    Backtick, // `
}

#[derive(Debug)]
pub enum TokkV {
    Operator(String),
    Quoted(QuoteType, String),
    // we're not using lines (we only need indental), but if anyone wants lines that'd make sense and they can have one
    // Line(Vec<Toast>)
    /// Invocation: paren type, optional caller (the token before the paren), and arguments
    Invocation(ParenType, Box<Toast>, Vec<Toast>),
    Indental {
        root_line: Vec<Toast>,
        indented: Vec<Toast>,
    },
}

/// Equality comparison for TokkV ignoring spans
impl PartialEq for TokkV {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (TokkV::Operator(a), TokkV::Operator(b)) => a == b,
            (TokkV::Quoted(qt1, s1), TokkV::Quoted(qt2, s2)) => qt1 == qt2 && s1 == s2,
            (TokkV::Invocation(pt1, c1, args1), TokkV::Invocation(pt2, c2, args2)) => {
                pt1 == pt2 && c1 == c2 && args1 == args2
            }
            (
                TokkV::Indental {
                    root_line: r1,
                    indented: i1,
                },
                TokkV::Indental {
                    root_line: r2,
                    indented: i2,
                },
            ) => r1 == r2 && i1 == i2,
            _ => false,
        }
    }
}

/// Equality comparison for Tokk ignoring span
impl PartialEq for Tokk {
    fn eq(&self, other: &Self) -> bool {
        self.content == other.content
    }
}

/// Tracks the content accumulated at a given indentation level
struct IndentLevel {
    /// Length of the indent prefix within the parent InvocationLevel's known_indent
    indent_len: usize,
    /// index of the first token on the final line
    line_start: usize,
    content: Vec<Toast>,
    /// Span start for this indent level
    span_start: usize,
}

/// Tracks open invocations (paren groups) and their associated indent state
struct InvocationLevel {
    /// None for root level, Some for actual parens
    paren_type: ParenType,
    /// The token that preceded the opening paren (the "caller"), if any
    caller: Toast,
    span_start: usize,
    /// The known indent string - extended as indent_stack grows, shortened as it pops
    known_indent: String,
    /// Indent stack for this invocation level
    indent_stack: Vec<IndentLevel>,
}

impl InvocationLevel {
    /// Get the current output destination within this invocation level.
    /// Uses `indented` if it's non-empty (meaning we've seen sub-content), otherwise `root_line`.
    fn end_list(&mut self) -> &mut Vec<Toast> {
        &mut self.indent_stack.last_mut().unwrap().content
    }

    fn pop_close_indent_level(&mut self) {
        // finalize level by collecting root_line and indented content
        let indented = self.indent_stack.pop().unwrap().content;
        let host = self.indent_stack.last_mut().unwrap();
        let root_line: Vec<_> = host.content.drain(host.line_start..).collect();

        if root_line.is_empty() {
            // No root line - just merge indented content into parent
            host.content.extend(indented);
        } else {
            // Create an Indental with root_line and indented content
            let span = {
                let start = root_line.first().unwrap().span().start;
                let end = indented
                    .last()
                    .map(|t| t.span().end())
                    .unwrap_or_else(|| root_line.last().unwrap().span().end());
                Span::from_range(start, end)
            };

            host.content.push(Toast::Tokk(Tokk {
                span,
                content: TokkV::Indental {
                    root_line,
                    indented,
                },
            }));
        }

        self.known_indent.truncate(host.indent_len);
    }

    /// Flush all indent levels, collapsing them into content
    /// called when the end of a paren level is reached
    fn pop_all_indents(&mut self, end_pos: usize) {
        while self.indent_stack.len() > 1 {
            self.pop_close_indent_level();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParenType {
    Round,
    Square,
    Curly,
}

impl ParenType {
    fn from_open(c: char) -> Self {
        match c {
            '(' => ParenType::Round,
            '[' => ParenType::Square,
            '{' => ParenType::Curly,
            _ => panic!("Invalid opening paren: {}", c),
        }
    }

    fn from_close(c: char) -> Self {
        match c {
            ')' => ParenType::Round,
            ']' => ParenType::Square,
            '}' => ParenType::Curly,
            _ => panic!("Invalid closing paren: {}", c),
        }
    }

    fn close_char(&self) -> char {
        match self {
            ParenType::Round => ')',
            ParenType::Square => ']',
            ParenType::Curly => '}',
        }
    }
}

#[derive(Debug)]
pub struct Error {
    pub span: Span,
    pub message: String,
}

impl Error {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }

    fn at_pos(pos: usize, message: impl Into<String>) -> Self {
        Self {
            span: Span::new(pos, 1),
            message: message.into(),
        }
    }
}

// ============================================================================
// Operator Precedence Lookup
// ============================================================================

/// Characters that can be part of operators (but may not be used in current config)
const OPERATOR_CHARS: &[char] = &[
    '=', ':', '+', '-', '*', '/', '<', '>', '!', '.', '&', '|', '^', ';', '~', '?',
];

/// Binding power for an operator - determines precedence and associativity.
/// For left-associative ops: l_bp < r_bp (e.g., + is (11, 12))
/// For right-associative ops: l_bp > r_bp (e.g., = is (2, 1))
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BindingInfo {
    pub left: u8,
    pub right: u8,
}
impl BindingInfo {
    /// Create a left-associative binding power at the given precedence level.
    /// Uses (level * 2 + 1, level * 2 + 2) to leave room for right-associative ops.
    pub fn left_assoc(precedence: u8) -> Self {
        Self {
            left: precedence * 2 + 1,
            right: precedence * 2 + 2,
        }
    }

    /// Create a right-associative binding power at the given precedence level.
    /// Uses (level * 2 + 2, level * 2 + 1) - reversed from left-associative.
    pub fn right_assoc(precedence: u8) -> Self {
        Self {
            left: precedence * 2 + 2,
            right: precedence * 2 + 1,
        }
    }

    /// Create a prefix unary operator (like `!` or `?`).
    /// Left is 0 (nothing to bind on left).
    pub fn prefix() -> Self {
        Self { left: 0, right: 0 }
    }

    pub fn is_prefix(&self) -> bool {
        self.left == 0 && self.right == 0
    }
}

/// Entry in the operator lookup table, indexed by first character of operator.
#[derive(Debug, Clone)]
pub enum OperatorEntry {
    /// This character can never be an operator (alphanumeric, whitespace, etc.)
    NonOperator,
    /// This character could be an operator but isn't used in the current operator list
    OperatorNotUsed,
    /// Exactly one operator starts with this character.
    /// Stores (full operator string, binding power).
    Single(String, BindingInfo),
    /// Multiple operators start with this character, need disambiguation.
    /// Stores vec of (full operator string, binding power).
    Contended(Vec<(String, BindingInfo)>),
}

/// Fast operator precedence lookup table.
/// Uses a 128-element array indexed by first character (ASCII).
#[derive(Debug, Clone)]
pub struct OperatorTable {
    entries: Vec<OperatorEntry>,
}

impl OperatorTable {
    /// Build an operator table from a list of (operator, binding_power) pairs.
    pub fn new(operators: Vec<(String, BindingInfo)>) -> Self {
        use std::collections::HashMap;

        // Initialize all entries as Alphanumeric
        let mut entries: Vec<OperatorEntry> = vec![OperatorEntry::NonOperator; 128];

        // Mark all potential operator characters as OperatorNotUsed
        for &c in OPERATOR_CHARS {
            let idx = c as usize;
            if idx < 128 {
                entries[idx] = OperatorEntry::OperatorNotUsed;
            }
        }

        // Group operators by their first character
        let mut by_first_char: HashMap<char, Vec<(String, BindingInfo)>> = HashMap::new();

        for (op, bp) in operators {
            if let Some(first_char) = op.chars().next() {
                by_first_char.entry(first_char).or_default().push((op, bp));
            }
        }

        // Convert grouped operators into entries
        for (c, ops) in by_first_char {
            let idx = c as usize;
            if idx < 128 {
                entries[idx] = if ops.len() == 1 {
                    let (op, bp) = ops.into_iter().next().unwrap();
                    OperatorEntry::Single(op, bp)
                } else {
                    OperatorEntry::Contended(ops)
                };
            }
        }

        Self { entries }
    }

    /// Build an operator table from a list of operators in precedence order.
    /// All operators are left-associative. Index 0 = lowest precedence.
    pub fn from_precedence_list(operators: Vec<String>) -> Self {
        let ops_with_bp: Vec<(String, BindingInfo)> = operators
            .into_iter()
            .enumerate()
            .map(|(prec, op)| (op, BindingInfo::left_assoc(prec as u8)))
            .collect();
        Self::new(ops_with_bp)
    }

    /// Look up an operator's binding power, or a default lowest binding power if it's an unrecognized operator.
    /// Returns None if the string doesn't consist of operator characters.
    pub fn lookup(&self, op: &str) -> Option<BindingInfo> {
        let first_char = op.chars().next()?;
        let idx = first_char as usize;

        if idx >= 128 {
            return None;
        }

        let found = match &self.entries[idx] {
            OperatorEntry::NonOperator => return None,
            OperatorEntry::OperatorNotUsed => None,
            OperatorEntry::Single(stored_op, bp) => {
                if stored_op == op {
                    Some(*bp)
                } else {
                    None
                }
            }
            OperatorEntry::Contended(ops) => ops.iter().find(|(s, _)| s == op).map(|(_, bp)| *bp),
        };
        // technically it doesn't need to check that all the chars are operator chars, if they weren't, it either would have early returned above, or the sequencing would have separated those into another token. But this code (unrecognized operators) is rarely run, it's otherwise an error.
        if found.is_none() && op.chars().all(|c| self.is_operator_char(c)) {
            // BindingPower(0, 1) means minimal precedence
            return Some(BindingInfo::left_assoc(0));
        }
        found
    }

    /// Check if a character is an operator character (could be part of an operator).
    /// This is true for OperatorNotUsed, Single, and Contended entries.
    #[inline]
    pub fn is_operator_char(&self, c: char) -> bool {
        let idx = c as usize;
        if idx >= 128 {
            return false;
        }
        !matches!(self.entries[idx], OperatorEntry::NonOperator)
    }

    /// Check if a character starts a registered operator (Single or Contended only).
    #[inline]
    pub fn is_registered_operator_start(&self, c: char) -> bool {
        let idx = c as usize;
        if idx >= 128 {
            return false;
        }
        matches!(
            self.entries[idx],
            OperatorEntry::Single(_, _) | OperatorEntry::Contended(_)
        )
    }
}

fn is_identifier_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_identifier_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

/// Create a Toast::Ast containing an Atom with the given value
fn make_atom(span: Span, value: String) -> Toast {
    Toast::Ast(ToastAst {
        span,
        v: ToastAstV::Atom { span, value },
    })
}

fn sequence(source: &str, operators: &OperatorTable) -> Result<Vec<Toast>, Vec<Error>> {
    use std::iter::Peekable;
    use std::str::Chars;

    let mut pos: usize = 0;
    let mut chars: Peekable<Chars> = source.chars().peekable();

    let mut errors: Vec<Error> = Vec::new();

    // Stack of open invocations - bottom entry is never popped
    let mut invocation_stack: Vec<InvocationLevel> = vec![InvocationLevel {
        caller: make_atom(Span::new(0, 0), "do".to_string()),
        paren_type: ParenType::Round,
        span_start: 0,
        known_indent: String::new(),
        indent_stack: vec![IndentLevel {
            indent_len: 0,
            line_start: 0,
            content: Vec::new(),
            span_start: 0,
        }],
    }];

    fn end_list(invocation_stack: &mut Vec<InvocationLevel>) -> &mut Vec<Toast> {
        &mut invocation_stack
            .last_mut()
            .unwrap()
            .indent_stack
            .last_mut()
            .unwrap()
            .content
    }

    // Track if we're at the start of a line (for indent processing)
    let mut at_line_start = true;

    'entire: while chars.peek().is_some() {
        // Handle line starts - measure indentation
        if at_line_start {
            let indent_start = pos;
            let current = invocation_stack.last_mut().unwrap();
            let base_len = current.indent_stack.last().unwrap().indent_len;

            // Read whitespace, comparing against known_indent and extending if deeper
            let mut new_len = 0;
            let mut indent_mismatch = false;

            loop {
                match chars.peek() {
                    Some(&' ') | Some(&'\t') => {
                        let c = *chars.peek().unwrap();
                        chars.next();
                        pos += 1;

                        if new_len < base_len {
                            // Compare against existing pattern
                            if current.known_indent.as_bytes()[new_len] != c as u8 {
                                indent_mismatch = true;
                            }
                        } else {
                            // Extending beyond base - add to known_indent
                            current.known_indent.push(c);
                        }
                        new_len += 1;
                    }
                    Some(&'\r') => {
                        // Empty line - consume and restart
                        chars.next();
                        pos += 1;
                        if chars.peek() == Some(&'\n') {
                            chars.next();
                            pos += 1;
                        }
                        // Reset any extension we made
                        current.known_indent.truncate(base_len);
                        new_len = 0;
                        indent_mismatch = false;
                    }
                    Some(&'\n') => {
                        // Empty line - consume and restart
                        chars.next();
                        pos += 1;
                        current.known_indent.truncate(base_len);
                        new_len = 0;
                        indent_mismatch = false;
                    }
                    _ => break,
                }
            }

            at_line_start = false;

            if chars.peek().is_none() {
                break;
            }

            // Check if the first non-whitespace char is a closing bracket, abandon indenting if so
            let next_char = chars.peek();
            let is_bracket = matches!(
                next_char,
                Some(')') | Some(']') | Some('}') | Some('(') | Some('[') | Some('{')
            );

            if is_bracket {
                // Bracket line - truncate any extension, ignore indent mismatch
                current.known_indent.truncate(base_len);
            } else {
                // Report indent mismatch now that we know it's not a bracket line
                if indent_mismatch {
                    errors.push(Error::new(
                        Span::new(indent_start, new_len.max(1)),
                        "Inconsistent indentation: whitespace pattern doesn't match previous levels",
                    ));
                }

                // Find if any level has indent_len <= new_len
                let is_recognized_prefix =
                    current.indent_stack.iter().any(|l| l.indent_len <= new_len);

                if !is_recognized_prefix && !indent_mismatch {
                    errors.push(Error::new(
                        Span::new(indent_start, new_len.max(1)),
                        "Inconsistent indentation: whitespace pattern doesn't match previous levels",
                    ));
                }

                // While new len is less than previous indent levels, pop them
                while let Some(top) = current.indent_stack.last()
                    && new_len < top.indent_len
                {
                    current.pop_close_indent_level();
                }

                // Check if we need to create a new indent level
                let should_push = if let Some(top) = current.indent_stack.last() {
                    new_len > top.indent_len
                } else {
                    true // No levels yet
                };

                if should_push {
                    current.indent_stack.push(IndentLevel {
                        indent_len: new_len,
                        line_start: 0,
                        content: Vec::new(),
                        span_start: pos,
                    });
                } else {
                    // Staying at same level - update line_start to mark new line
                    if let Some(top) = current.indent_stack.last_mut() {
                        top.line_start = top.content.len();
                    }
                }
            }
        }

        let Some(&c) = chars.peek() else { break };

        match c {
            // Newline handling
            '\n' => {
                chars.next();
                pos += 1;
                at_line_start = true;
            }
            '\r' => {
                chars.next();
                pos += 1;
                if chars.peek() == Some(&'\n') {
                    chars.next();
                    pos += 1;
                }
                at_line_start = true;
            }

            // Whitespace
            ' ' | '\t' => {
                chars.next();
                pos += 1;
            }

            // Opening brackets - start an invocation
            '(' | '[' | '{' => {
                let paren_type = ParenType::from_open(c);

                // Steal the previous token (if any) as the caller
                let mut caller: Toast = match end_list(&mut invocation_stack).pop() {
                    Some(t) => t,
                    None => {
                        errors.push(Error::new(
                            Span::new(pos, 1),
                            format!("Opening '{}' requires a caller - nothing precedes it", c),
                        ));
                        make_atom(Span::new(pos, 1), "ERROR_DUMMY_TOKEN".into())
                    }
                };

                // there used to be code for looking up an indent level, but this actually can't work, an indent wouldn't be created if the opening thing were a paren, per the above.

                // Adjust span_start to include the caller if present
                let span_start = caller.span().start;

                invocation_stack.push(InvocationLevel {
                    paren_type: paren_type,
                    caller,
                    span_start,
                    known_indent: String::new(),
                    indent_stack: vec![IndentLevel {
                        indent_len: 0,
                        line_start: 0,
                        content: Vec::new(),
                        span_start: pos,
                    }],
                });

                chars.next();
                pos += 1;
            }

            // Closing brackets - complete an invocation
            ')' | ']' | '}' => {
                let expected_type = ParenType::from_close(c);

                // Check we're not trying to close the root
                if invocation_stack.len() <= 1 {
                    errors.push(Error::new(
                        Span::new(pos, 1),
                        format!("Unmatched closing bracket: '{}'", c),
                    ));
                    break;
                }

                // Check for matching paren type
                let paren_type = invocation_stack.last().unwrap().paren_type; // Safe: not root
                if paren_type != expected_type {
                    let expected_char = paren_type.close_char();
                    errors.push(Error::new(
                        Span::new(pos, 1),
                        format!(
                            "Mismatched bracket: expected '{}', found '{}'",
                            expected_char, c
                        ),
                    ));
                    // and then I guess we just keep going...?
                    // yeah I mean if someone closes this paren type later on that's a match and should be recognized... maybe? I dunno, it could produce a lot of very confusing errors, maybe it's bad design to show those. Okay fine I'll break.
                    // continue;
                    break;
                }

                // Flush any remaining indents inside this invocation
                let mut invocation_level = invocation_stack.pop().unwrap();
                invocation_level.pop_all_indents(pos);

                let InvocationLevel {
                    paren_type,
                    caller,
                    mut indent_stack,
                    ..
                } = invocation_level;
                let content = indent_stack.drain(..).next().unwrap().content;

                // Create the Invocation tokk
                let tokk = Tokk {
                    span: Span::from_range(invocation_level.span_start, pos + 1),
                    content: TokkV::Invocation(paren_type, Box::new(caller), content),
                };

                // Add to parent's output
                let parent = invocation_stack.last_mut().unwrap();
                parent.end_list().push(Toast::Tokk(tokk));

                chars.next();
                pos += 1;
            }

            // Quoted literals: single, double, backtick
            '"' | '\'' | '`' => {
                let quote_char = c;
                let quote_type = match c {
                    '\'' => QuoteType::Single,
                    '"' => QuoteType::Double,
                    '`' => QuoteType::Backtick,
                    _ => unreachable!(),
                };
                let start = pos;
                chars.next();
                pos += 1;

                let mut value = String::new();
                loop {
                    let Some(&ch) = chars.peek() else {
                        errors.push(Error::new(
                            Span::from_range(start, pos),
                            format!("Unclosed {} string literal", quote_char),
                        ));
                        break;
                    };

                    if ch == quote_char {
                        chars.next();
                        pos += 1;
                        break;
                    }

                    match ch {
                        '\\' => {
                            chars.next();
                            pos += 1;
                            let Some(&escaped) = chars.peek() else {
                                errors.push(Error::new(
                                    Span::from_range(start, pos),
                                    format!("Unclosed {} string literal", quote_char),
                                ));
                                break;
                            };
                            let escaped_char = match escaped {
                                'n' => '\n',
                                'r' => '\r',
                                't' => '\t',
                                '\\' => '\\',
                                '\'' => '\'',
                                '"' => '"',
                                '`' => '`',
                                '0' => '\0',
                                _ => {
                                    errors.push(Error::new(
                                        Span::new(pos, 1),
                                        format!("Invalid escape sequence: \\{}", escaped),
                                    ));
                                    escaped
                                }
                            };
                            value.push(escaped_char);
                            chars.next();
                            pos += 1;
                        }
                        '\n' => {
                            value.push(ch);
                            chars.next();
                            pos += 1;
                        }
                        '\r' => {
                            chars.next();
                            pos += 1;
                            if chars.peek() == Some(&'\n') {
                                chars.next();
                                pos += 1;
                            }
                            value.push('\n');
                        }
                        _ => {
                            value.push(ch);
                            chars.next();
                            pos += 1;
                        }
                    }
                }

                let tokk = Tokk {
                    span: Span::from_range(start, pos),
                    content: TokkV::Quoted(quote_type, value),
                };
                end_list(&mut invocation_stack).push(Toast::Tokk(tokk));
            }

            // Check for operators
            _ if operators.is_operator_char(c) => {
                let start = pos;
                let mut op = String::new();

                while let Some(&ch) = chars.peek() {
                    if !operators.is_operator_char(ch) {
                        break;
                    }
                    op.push(ch);
                    chars.next();
                    pos += 1;
                }

                let tokk = Tokk {
                    span: Span::from_range(start, pos),
                    content: TokkV::Operator(op),
                };
                let current = invocation_stack.last_mut().unwrap();
                current.end_list().push(Toast::Tokk(tokk));
            }

            // Comment
            '#' => {
                let start = pos;
                chars.next();
                pos += 1;

                if chars.peek() == Some(&'(') {
                    // Multi-line comment #(...)
                    chars.next();
                    pos += 1;
                    let mut depth = 1;
                    let mut content = String::from("#(");

                    while depth > 0 {
                        let Some(ch) = chars.next() else {
                            errors.push(Error::new(
                                Span::from_range(start, pos),
                                "Unclosed multi-line comment",
                            ));
                            break 'entire;
                        };
                        pos += 1;
                        content.push(ch);
                        match ch {
                            '(' => depth += 1,
                            ')' => depth -= 1,
                            '\r' => {
                                if chars.peek() == Some(&'\n') {
                                    chars.next();
                                    pos += 1;
                                    content.push('\n');
                                }
                            }
                            _ => {}
                        }
                    }

                    let span = Span::from_range(start, pos);
                    let comment = Toast::Ast(ToastAst {
                        span,
                        v: ToastAstV::Comment { span, content },
                    });
                    end_list(&mut invocation_stack).push(comment);
                } else {
                    // Single-line comment
                    let mut content = String::from("#");
                    while let Some(&ch) = chars.peek() {
                        if ch == '\n' || ch == '\r' {
                            break;
                        }
                        content.push(ch);
                        chars.next();
                        pos += 1;
                    }

                    let span = Span::from_range(start, pos);
                    let comment = Toast::Ast(ToastAst {
                        span,
                        v: ToastAstV::Comment { span, content },
                    });
                    end_list(&mut invocation_stack).push(comment);
                }
            }

            // Regular token (identifier, number, etc.)
            _ => {
                let start = pos;
                let mut value = String::new();

                while let Some(&ch) = chars.peek() {
                    // Stop at whitespace, brackets, operators, quotes, or comments
                    if ch.is_whitespace()
                        || ch == '('
                        || ch == ')'
                        || ch == '['
                        || ch == ']'
                        || ch == '{'
                        || ch == '}'
                        || ch == '"'
                        || ch == '\''
                        || ch == '`'
                        || ch == '#'
                        || operators.is_operator_char(ch)
                    {
                        break;
                    }
                    value.push(ch);
                    chars.next();
                    pos += 1;
                }

                if !value.is_empty() {
                    let atom = make_atom(Span::from_range(start, pos), value);
                    end_list(&mut invocation_stack).push(atom);
                }
            }
        }
    }

    // Check for unclosed parens (any beyond the root)
    if invocation_stack.len() > 1 {
        let entry = &invocation_stack.last().unwrap();
        let paren_char = entry.paren_type.close_char();
        errors.push(Error::new(
            Span::new(entry.span_start, 1),
            format!("Unclosed bracket: '{}'", paren_char),
        ));
    }

    if !errors.is_empty() {
        Err(errors)
    } else {
        // Collapse any remaining indent levels into Indentals
        let mut root = invocation_stack.drain(..).next().unwrap();
        root.pop_all_indents(pos);
        // Return the content of the base indent level
        Ok(root.indent_stack.drain(..).next().unwrap().content)
    }
}

/// Token Or Ast
#[derive(Debug)]
pub enum Toast {
    Tokk(Tokk),
    Ast(ToastAst),
}

/// Equality comparison for Toast ignoring spans
impl PartialEq for Toast {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Toast::Tokk(a), Toast::Tokk(b)) => a == b,
            (Toast::Ast(a), Toast::Ast(b)) => a == b,
            _ => false,
        }
    }
}

impl Toast {
    fn as_tokk(&self) -> &Tokk {
        match self {
            Toast::Tokk(tokk) => tokk,
            Toast::Ast(ast) => panic!("Toast is not a Tokk"),
        }
    }
    /// makes sure every toast is an ast
    fn verify_ast(&self) -> Result<&Toast, Vec<Error>> {
        let mut errors = Vec::new();
        self.verify_ast_writer(&mut errors);
        if errors.is_empty() {
            Ok(self)
        } else {
            Err(errors)
        }
    }
    fn verify_ast_writer(&self, errors: &mut Vec<Error>) {
        match self {
            Toast::Tokk(tokk) => errors.push(Error::new(tokk.span, "Toast is not an Ast")),
            Toast::Ast(ast) => match &ast.v {
                ToastAstV::Invocation { parameters, .. } => {
                    for p in parameters {
                        p.verify_ast_writer(errors);
                    }
                }
                ToastAstV::Conditional {
                    condition,
                    then,
                    elsen,
                    elsifs,
                    ..
                } => {
                    condition.verify_ast_writer(errors);
                    then.verify_ast_writer(errors);
                    if let Some(e) = elsen {
                        e.verify_ast_writer(errors);
                    }
                    for (c, b) in elsifs {
                        c.verify_ast_writer(errors);
                        b.verify_ast_writer(errors);
                    }
                }
                ToastAstV::Function {
                    args,
                    return_type,
                    body,
                    ..
                } => {
                    for arg in args {
                        arg.verify_ast_writer(errors);
                    }
                    if let Some(r) = return_type {
                        r.verify_ast_writer(errors);
                    }
                    body.verify_ast_writer(errors);
                }
                ToastAstV::Block { statements, .. } => {
                    for st in statements {
                        st.verify_ast_writer(errors);
                    }
                }
                ToastAstV::Operator { arguments, .. } => {
                    for a in arguments {
                        a.verify_ast_writer(errors);
                    }
                }
                _ => {}
            },
        }
    }
    fn span(&self) -> Span {
        match self {
            Toast::Tokk(tokk) => tokk.span,
            Toast::Ast(ast) => ast.span,
        }
    }
}

#[derive(Debug)]
pub struct ToastAst {
    pub span: Span,
    pub v: ToastAstV,
}

#[derive(Debug)]
pub enum ToastAstV {
    Invocation {
        span: Span,
        head: String,
        kind: ParenType,
        parameters: Vec<Box<Toast>>,
    },
    Comment {
        span: Span,
        content: String,
    },
    Conditional {
        span: Span,
        condition: Box<Toast>,
        then: Box<Toast>,
        elsen: Option<Box<Toast>>,
        elsifs: Vec<(Box<Toast>, Box<Toast>)>,
    },
    Function {
        span: Span,
        args: Vec<Box<Toast>>,
        return_type: Option<Box<Toast>>,
        body: Box<Toast>,
    },
    Block {
        span: Span,
        statements: Vec<Box<Toast>>,
    },
    Atom {
        span: Span,
        value: String,
    },
    Quoted {
        span: Span,
        quote_type: QuoteType,
        value: String,
    },
    Operator {
        span: Span,
        operator: String,
        arguments: Vec<Box<Toast>>,
    },
}

/// Equality comparison for ToastAst ignoring span
impl PartialEq for ToastAst {
    fn eq(&self, other: &Self) -> bool {
        self.v == other.v
    }
}

/// Equality comparison for ToastAstV ignoring spans
impl PartialEq for ToastAstV {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                ToastAstV::Invocation {
                    head: h1,
                    kind: k1,
                    parameters: p1,
                    ..
                },
                ToastAstV::Invocation {
                    head: h2,
                    kind: k2,
                    parameters: p2,
                    ..
                },
            ) => h1 == h2 && k1 == k2 && p1 == p2,
            (ToastAstV::Comment { content: c1, .. }, ToastAstV::Comment { content: c2, .. }) => {
                c1 == c2
            }
            (
                ToastAstV::Conditional {
                    condition: c1,
                    then: t1,
                    elsen: e1,
                    elsifs: ei1,
                    ..
                },
                ToastAstV::Conditional {
                    condition: c2,
                    then: t2,
                    elsen: e2,
                    elsifs: ei2,
                    ..
                },
            ) => c1 == c2 && t1 == t2 && e1 == e2 && ei1 == ei2,
            (
                ToastAstV::Function {
                    args: a1,
                    return_type: r1,
                    body: b1,
                    ..
                },
                ToastAstV::Function {
                    args: a2,
                    return_type: r2,
                    body: b2,
                    ..
                },
            ) => a1 == a2 && r1 == r2 && b1 == b2,
            (ToastAstV::Block { statements: s1, .. }, ToastAstV::Block { statements: s2, .. }) => {
                s1 == s2
            }
            (ToastAstV::Atom { value: v1, .. }, ToastAstV::Atom { value: v2, .. }) => v1 == v2,
            (
                ToastAstV::Quoted {
                    quote_type: q1,
                    value: v1,
                    ..
                },
                ToastAstV::Quoted {
                    quote_type: q2,
                    value: v2,
                    ..
                },
            ) => q1 == q2 && v1 == v2,
            (
                ToastAstV::Operator {
                    operator: o1,
                    arguments: a1,
                    ..
                },
                ToastAstV::Operator {
                    operator: o2,
                    arguments: a2,
                    ..
                },
            ) => o1 == o2 && a1 == a2,
            _ => false,
        }
    }
}

/**
the astrule step takes a sequence of [Toast]s that are initially all [Tokk]s and applies some rewrite rules to transform them into [Ast] Toasts, which are then stripped down into a tree of pure Asts.
`%endfirst` means it's greedy but from the other direction, processing terms from the right first (this may be what lazy means in general for all I know, but I think endfirst is a much clearer term for this, it means it'll be intuitive if we also use this for right-associativity)
The macro matching rule syntax here is pretty much taken from rust.

# backtick quotes just translate to an atom, ie, we use them to allow expressing symbols with spaces in them.
for $x:Quoted(Backticked) → ast::Atom($x)
# tokens are just atoms
for $x:Token → ToastAst::Atom($x)

# operators

def %indenter = fn | if | do
%indental(%endfirst($b* $s:%indenter $a*))($d*) → $b %indental($s $a)($d)

%indental($o:operators $(x)?)($(y)*) → $o($x $y)

#if there are operator-llinked things within an indental head with a non-operator term at the end, the indental belongs to that non-operator stuff at the end
%indental(a@$($_ $_@operators)+ $y*)($z*) → $a %indental($y)($z)

def %operatorExpression = $($_ $_@operators)+ $f
# inline if else (ternary)
if($x*) else($y*) → if($x else $y)
if $c@operatorExpression $x@operatorExpression else $y@operatorExpression → if($c $x else $y)

fn $p* to:$rt? $b@operatorExpression → fn($p* to$(:$rt)? $b)

# convert all infix operator expressions to invocation asts
to ast::Invocation(head($o) arguments($x $y))
    $x $o@operators $y
    %indental($x* $o@operators)($(y)*)


if($c $x* else $y*) → ast::Conditional(condition:$c then:[$x] elsen:[$y])

# normalize conditional parts
to if($c $x)
    if $c $x
    %indental(if $c*)($x*)
to elif($c $x)
    elif $c $x
    %indental(elif $c)($x*)
to else($x)
    else $x
    %indental(else)($x*)

if($c $x*) $(elif($ce $xe))* $(else ($xe))? → ast::Conditional(condition:$c then:$x elsen:$(else ($xe))? elsifs:[$(elif($ce $xe))*])

%indental(do $predoings*)($doings*) → do($predoings* $doings*)
do($doings*) → Ast::Block(statements:[$doings*])

to ast::Function(parameters:[$parameters] body:[$doings])
    fn($parameters* to $doings*)
    %indental(fn $parameters* $(to)?)($doings*)
    fn $parameters* to $doings
to ast::Function(parameters:[$parameters*] returnType:$return body:[$doings*])
    fn($parameters* :(to $return) $doings*)
    %indental(fn $parameters* to $(:)? $return)($doings*)
    fn $parameters* :(to $return) $doings

# all remaining indentals are invocations
%indental($x $xs*)($y*) → ast::Invocation(head:$x arguments:[$xs* $y*])
%indental($x $xs* do)($y*) → ast::Invocation(head:$x arguments:[$xs* do($y*)])


After this, in Result, all [Toast]s should be [ToastAst]s, no [Tokk]s should remain.
*/

// converts every toast into a ToastAst, or reports errors
fn structure(
    tokks: &mut Vec<Toast>,
    operators: &[String],
    operator_table: &OperatorTable,
) -> Result<(), Vec<Error>> {
    let mut errors = Vec::new();
    structure_series(tokks, operators, operator_table, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// status: I'm a bit burned out. It turns out that it's not going to be efficient to apply rules one after the other, so the conversion algorithm is going to be a lot less elegant than the spec, and proving that the algorithm agrees with the spec is cognitively draining.
// well, If I can prove to myself that the faster code version of this is equivalent to applying rewrite rules in multiple passes, then I'm pretty sure that'd imply that a sufficiently smart rule applyer could also produce that code, so even if we don't have that now, it can probably be found later. So it's fine.

fn structure_series(
    toasts: &mut Vec<Toast>,
    operators: &[String],
    operator_table: &OperatorTable,
    errors: &mut Vec<Error>,
) {
    for toast in toasts {
        structure_individual(toast, operators, operator_table, errors);
    }
    // TODO: look for operators and if else sequences (Pratt parsing goes here)
}
fn structure_individual(
    toast: &mut Toast,
    operators: &[String],
    operator_table: &OperatorTable,
    errors: &mut Vec<Error>,
) {
    match toast {
        Toast::Tokk(tokk) => {
            // Convert Tokk to ToastAst based on the rules
            let span = tokk.span;
            let new_ast = match &mut tokk.content {
                // Operators stay as operators (will be processed by Pratt parsing)
                TokkV::Operator(s) => ToastAst {
                    span,
                    v: ToastAstV::Operator {
                        span,
                        operator: take(s),
                        arguments: Vec::new(),
                    },
                },
                // Backtick quotes become atoms: for $x:Quoted(Backticked) → ast::Atom($x)
                TokkV::Quoted(QuoteType::Backtick, s) => ToastAst {
                    span,
                    v: ToastAstV::Atom {
                        span,
                        value: take(s),
                    },
                },
                // Other quotes stay as quoted
                TokkV::Quoted(quote_type, s) => ToastAst {
                    span,
                    v: ToastAstV::Quoted {
                        span,
                        quote_type: *quote_type,
                        value: take(s),
                    },
                },
                // Invocations: convert to ast::Invocation and recurse into arguments
                TokkV::Invocation(paren_type, caller, args) => {
                    let head = match caller.as_ref() {
                        Toast::Ast(ToastAst { v: ToastAstV::Atom { value, .. }, .. }) => value.clone(),
                        _ => String::new(),
                    };
                    let kind = *paren_type;
                    let mut parameters: Vec<Box<Toast>> =
                        take(args).into_iter().map(Box::new).collect();
                    // Recurse into each parameter
                    for param in &mut parameters {
                        structure_individual(param, operators, operator_table, errors);
                    }
                    ToastAst {
                        span,
                        v: ToastAstV::Invocation {
                            span,
                            head,
                            kind,
                            parameters,
                        },
                    }
                }
                // Indentals: these need special handling based on the rules
                TokkV::Indental {
                    root_line,
                    indented,
                } => {
                    // For now, convert to an invocation with root_line as head context
                    // and indented as body - this matches %indental patterns
                    let mut root_params: Vec<Box<Toast>> =
                        take(root_line).into_iter().map(Box::new).collect();
                    let mut indented_params: Vec<Box<Toast>> =
                        take(indented).into_iter().map(Box::new).collect();

                    // Recurse into root_line and indented toasts
                    for param in &mut root_params {
                        structure_individual(param, operators, operator_table, errors);
                    }
                    for param in &mut indented_params {
                        structure_individual(param, operators, operator_table, errors);
                    }

                    // Combine all parameters - indental becomes an invocation
                    let mut all_params = root_params;
                    all_params.extend(indented_params);

                    // Try to extract head from first token if available
                    let head = all_params
                        .first()
                        .and_then(|p| match p.as_ref() {
                            Toast::Ast(ToastAst {
                                v: ToastAstV::Atom { value, .. },
                                ..
                            }) => Some(value.clone()),
                            _ => None,
                        })
                        .unwrap_or_default();

                    ToastAst {
                        span,
                        v: ToastAstV::Invocation {
                            span,
                            head,
                            kind: ParenType::Round,
                            parameters: all_params,
                        },
                    }
                }
            };
            *toast = Toast::Ast(new_ast);
        }
        Toast::Ast(ast) => {
            // no conversion, just recurses
            match &mut ast.v {
                ToastAstV::Invocation { parameters, .. } => {
                    for param in parameters {
                        structure_individual(param, operators, operator_table, errors);
                    }
                }
                ToastAstV::Conditional {
                    condition,
                    then,
                    elsen,
                    elsifs,
                    ..
                } => {
                    structure_individual(condition, operators, operator_table, errors);
                    structure_individual(then, operators, operator_table, errors);
                    if let Some(e) = elsen {
                        structure_individual(e, operators, operator_table, errors);
                    }
                    for (cond, body) in elsifs {
                        structure_individual(cond, operators, operator_table, errors);
                        structure_individual(body, operators, operator_table, errors);
                    }
                }
                ToastAstV::Function {
                    args,
                    return_type,
                    body,
                    ..
                } => {
                    for arg in args {
                        structure_individual(arg, operators, operator_table, errors);
                    }
                    if let Some(ret) = return_type {
                        structure_individual(ret, operators, operator_table, errors);
                    }
                    structure_individual(body, operators, operator_table, errors);
                }
                ToastAstV::Block { statements, .. } => {
                    for stmt in statements {
                        structure_individual(stmt, operators, operator_table, errors);
                    }
                }
                ToastAstV::Operator { arguments, .. } => {
                    for arg in arguments {
                        structure_individual(arg, operators, operator_table, errors);
                    }
                }
                // Leaf nodes - no children to recurse into
                ToastAstV::Comment { .. } | ToastAstV::Atom { .. } | ToastAstV::Quoted { .. } => {}
            }
        }
    }
}

// struct Alteration<'a> {
//     remove: Vec<TastID>,
//     replacement: Box<Ast>,
// }

/// matches against any of the keywords
struct KeywordRule {
    keywords: Vec<String>,
    rule: fn(t: &mut Toast) -> Result<Toast, Error>,
}

/// operators are ordered from highest to lowest precedence
// fn parse(content: &str, operators:&[String], rules: &[KeywordRule]) -> Result<Arena, Error> {
//     let operator_table = OperatorTable::new(operators.to_vec());
//     let tokks = sequence(content, &operator_table)?;
//     astrules(tokks, operators, &operator_table, &rules)
// }

// fn parse_language(content: &str)-> Result<Arena, Error> {
//     parse(content, &[
//         "=".into(),
//         "||".into(),
//         "&&".into(),
//         "!=".into(),
//         "==".into(),
//         "+".into(),
//         "-".into(),
//         "*".into(),
//         "/".into(),
//         ">".into(),
//         "<".into(),
//         ">=".into(),
//         "<=".into(),
//         ":".into(),
//         ".".into(),
//     ],
//     &[])
// }

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Tokenizer Tests
    // ========================================================================

    /// Helper to extract atom value or operator string from Toast
    fn as_token(toast: &Toast) -> Option<&str> {
        match toast {
            Toast::Ast(ToastAst { v: ToastAstV::Atom { value, .. }, .. }) => Some(value),
            Toast::Tokk(Tokk { content: TokkV::Operator(s), .. }) => Some(s),
            _ => None,
        }
    }

    /// Helper to extract comment content from Toast
    fn as_comment(toast: &Toast) -> Option<&str> {
        match toast {
            Toast::Ast(ToastAst { v: ToastAstV::Comment { content, .. }, .. }) => Some(content),
            _ => None,
        }
    }

    /// Helper to extract operator string from Toast
    fn as_operator(tokk: &Toast) -> Option<&str> {
        match &tokk {
            Toast::Tokk(Tokk {
                content: TokkV::Operator(s),
                ..
            }) => Some(s),
            _ => None,
        }
    }

    /// Helper to extract operator string from Tokk directly
    fn as_operator_from_tokk(tokk: &Tokk) -> Option<&str> {
        match &tokk.content {
            TokkV::Operator(s) => Some(s),
            _ => None,
        }
    }

    /// Helper to extract quoted string from tokk
    fn as_quoted(tokk: &Toast) -> Option<(QuoteType, &str)> {
        match &tokk {
            Toast::Tokk(Tokk {
                content: TokkV::Quoted(qt, s),
                ..
            }) => Some((*qt, s)),
            _ => None,
        }
    }

    /// Helper to extract invocation content from tokk
    fn as_invocation(tokk: &Toast) -> (ParenType, &Toast, &Vec<Toast>) {
        match &tokk {
            Toast::Tokk(Tokk {
                content: TokkV::Invocation(pt, caller, content),
                ..
            }) => (*pt, caller, &content),
            _ => panic!("Toast is not an Invocation"),
        }
    }

    /// Helper to extract indental from tokk
    fn as_indental(tokk: &Toast) -> Option<(&Vec<Toast>, &Vec<Toast>)> {
        match &tokk {
            Toast::Tokk(Tokk {
                content:
                    TokkV::Indental {
                        root_line,
                        indented,
                    },
                ..
            }) => Some((root_line, indented)),
            _ => None,
        }
    }

    fn make_operator_table() -> OperatorTable {
        OperatorTable::new(vec![
            // Assignment (lowest precedence, right-associative)
            ("=".into(), BindingInfo::right_assoc(1)),
            ("+=".into(), BindingInfo::right_assoc(1)),
            ("-=".into(), BindingInfo::right_assoc(1)),
            ("*=".into(), BindingInfo::right_assoc(1)),
            ("/=".into(), BindingInfo::right_assoc(1)),
            ("??=".into(), BindingInfo::right_assoc(1)),
            // Null coalescing
            ("??".into(), BindingInfo::right_assoc(2)),
            // Logical
            ("||".into(), BindingInfo::left_assoc(3)),
            ("&&".into(), BindingInfo::left_assoc(4)),
            // Equality
            ("==".into(), BindingInfo::left_assoc(5)),
            ("!=".into(), BindingInfo::left_assoc(5)),
            // Comparison
            ("<".into(), BindingInfo::left_assoc(6)),
            (">".into(), BindingInfo::left_assoc(6)),
            ("<=".into(), BindingInfo::left_assoc(6)),
            (">=".into(), BindingInfo::left_assoc(6)),
            // Additive
            ("+".into(), BindingInfo::left_assoc(7)),
            ("-".into(), BindingInfo::left_assoc(7)),
            // Multiplicative
            ("*".into(), BindingInfo::left_assoc(8)),
            ("/".into(), BindingInfo::left_assoc(8)),
            // Type annotation
            (":".into(), BindingInfo::left_assoc(9)),
            // Optional chaining
            ("??.".into(), BindingInfo::left_assoc(10)),
            (".?".into(), BindingInfo::left_assoc(10)),
            // Member access (highest binary precedence)
            (".".into(), BindingInfo::left_assoc(11)),
            // Unary prefix
            ("!".into(), BindingInfo::prefix()),
            ("?".into(), BindingInfo::prefix()),
        ])
    }
    fn make_token(t: &str) -> Toast {
        make_atom(Span::new(0, t.len()), t.into())
    }
    fn make_indented(root_line: Vec<Toast>, indented: Vec<Toast>) -> Toast {
        Toast::Tokk(Tokk {
            span: Span::new(0, 0), // ignored by PartialEq
            content: TokkV::Indental {
                root_line,
                indented,
            },
        })
    }

    #[test]
    fn test_tokenize_simple_tokens() {
        let ops = make_operator_table();
        let tokks = sequence("hello world", &ops).expect("should tokenize");
        assert_eq!(tokks.len(), 2);
        assert_eq!(as_token(&tokks[0]), Some("hello"));
        assert_eq!(as_token(&tokks[1]), Some("world"));
    }

    #[test]
    fn test_tokenize_operators_separate_tokens() {
        let ops = make_operator_table();
        let tokks = sequence("a+b", &ops).expect("should tokenize");
        assert_eq!(tokks.len(), 3);
        assert_eq!(as_token(&tokks[0]), Some("a"));
        assert_eq!(as_token(&tokks[1]), Some("+"));
        assert_eq!(as_token(&tokks[2]), Some("b"));
    }

    #[test]
    fn test_tokenize_parens() {
        let ops = make_operator_table();
        let tokks = sequence("foo(bar)", &ops).expect("should tokenize");
        // foo(bar) is a single invocation with foo as caller
        assert_eq!(tokks.len(), 1);

        let (paren_type, caller, content) = as_invocation(&tokks[0]);
        assert!(matches!(paren_type, ParenType::Round));
        assert_eq!(as_token(caller), Some("foo"));
        assert_eq!(content.len(), 1);
        assert_eq!(as_token(&content[0]), Some("bar"));
    }

    #[test]
    fn test_tokenize_square_brackets() {
        let ops = make_operator_table();
        let tokks = sequence("arr[0]", &ops).expect("should tokenize");
        // arr[0] is a single invocation with arr as caller
        assert_eq!(tokks.len(), 1);

        let (paren_type, caller, content) = as_invocation(&tokks[0]);
        assert!(matches!(paren_type, ParenType::Square));
        assert_eq!(as_token(caller), Some("arr"));
        assert_eq!(content.len(), 1);
        assert_eq!(as_token(&content[0]), Some("0"));
    }

    #[test]
    fn test_tokenize_curly_brackets_no_caller() {
        let ops = make_operator_table();
        // Curly bracket without caller should be an error
        assert!(sequence("{a b}", &ops).is_err());
    }

    #[test]
    fn test_tokenize_mismatched_parens() {
        let ops = make_operator_table();
        let result = sequence("(a]", &ops);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.iter().any(|e| e.message.contains("Mismatched")));
    }

    #[test]
    fn test_tokenize_unclosed_paren() {
        let ops = make_operator_table();
        let result = sequence("(a b", &ops);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.iter().any(|e| e.message.contains("Unclosed")));
    }

    #[test]
    fn test_tokenize_unmatched_close() {
        let ops = make_operator_table();
        let result = sequence("a)", &ops);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.iter().any(|e| e.message.contains("Unmatched")));
    }

    #[test]
    fn test_tokenize_double_quoted_string() {
        let ops = make_operator_table();
        let scrips = sequence(r#""hello world""#, &ops).expect("should tokenize");
        assert_eq!(scrips.len(), 1);
        let (qt, s) = as_quoted(&scrips[0]).expect("should be quoted");
        assert_eq!(qt, QuoteType::Double);
        assert_eq!(s, "hello world");
    }

    #[test]
    fn test_tokenize_single_quoted_string() {
        let ops = make_operator_table();
        let scrips = sequence("'hello world'", &ops).expect("should tokenize");
        assert_eq!(scrips.len(), 1);
        let (qt, s) = as_quoted(&scrips[0]).expect("should be quoted");
        assert_eq!(qt, QuoteType::Single);
        assert_eq!(s, "hello world");
    }

    #[test]
    fn test_tokenize_backtick_string() {
        let ops = make_operator_table();
        let scrips = sequence("`hello world`", &ops).expect("should tokenize");
        assert_eq!(scrips.len(), 1);
        let (qt, s) = as_quoted(&scrips[0]).expect("should be quoted");
        assert_eq!(qt, QuoteType::Backtick);
        assert_eq!(s, "hello world");
    }

    #[test]
    fn test_tokenize_string_escape() {
        let ops = make_operator_table();
        let scrips = sequence(r#""hello\nworld""#, &ops).expect("should tokenize");
        assert_eq!(scrips.len(), 1);
        let (qt, s) = as_quoted(&scrips[0]).expect("should be quoted");
        assert_eq!(qt, QuoteType::Double);
        assert_eq!(s, "hello\nworld");
    }

    #[test]
    fn test_tokenize_escape_quote_in_string() {
        let ops = make_operator_table();
        // Test escaping the quote char itself
        let scrips = sequence(r#""say \"hi\"""#, &ops).expect("should tokenize");
        assert_eq!(scrips.len(), 1);
        let (_, s) = as_quoted(&scrips[0]).expect("should be quoted");
        assert_eq!(s, "say \"hi\"");

        let scrips = sequence(r"'it\'s'", &ops).expect("should tokenize");
        assert_eq!(scrips.len(), 1);
        let (qt, s) = as_quoted(&scrips[0]).expect("should be quoted");
        assert_eq!(qt, QuoteType::Single);
        assert_eq!(s, "it's");
    }

    #[test]
    fn test_tokenize_comment() {
        let ops = make_operator_table();
        let scrips = sequence("a # this is a comment\nb", &ops).expect("should tokenize");
        assert_eq!(scrips.len(), 3);
        assert_eq!(as_token(&scrips[0]), Some("a"));
        assert!(as_comment(&scrips[1]).unwrap().starts_with("#"));
        assert_eq!(as_token(&scrips[2]), Some("b"));
    }

    #[test]
    fn test_tokenize_multiline_comment() {
        let ops = make_operator_table();
        let scrips = sequence("a #(multi\nline) b", &ops).expect("should tokenize");
        assert_eq!(scrips.len(), 3);
        assert_eq!(as_token(&scrips[0]), Some("a"));
        let comment = as_comment(&scrips[1]).unwrap();
        assert!(comment.starts_with("#("));
        assert!(comment.contains("multi"));
        assert_eq!(as_token(&scrips[2]), Some("b"));
    }

    #[test]
    fn test_tokenize_nested_parens() {
        let ops = make_operator_table();
        let scrips = sequence("f(g(x))", &ops).expect("should tokenize");
        // f(g(x)) is one invocation: caller=f, content=[g(x)]
        assert_eq!(scrips.len(), 1);

        let (_, outer_caller, outer_content) = as_invocation(&scrips[0]);
        assert_eq!(as_token(outer_caller), Some("f"));
        // Inside f(...) we have g(x) which is one invocation
        assert_eq!(outer_content.len(), 1);

        let (_, inner_caller, inner_content) = as_invocation(&outer_content[0]);
        assert_eq!(as_token(inner_caller), Some("g"));
        assert_eq!(inner_content.len(), 1);
        assert_eq!(as_token(&inner_content[0]), Some("x"));
    }

    #[test]
    fn test_tokenize_indentation_basic() {
        let ops = make_operator_table();
        let scrips = sequence("foo\n  bar\n  baz", &ops).expect("should tokenize");

        // Should produce an Indental with foo as root and bar, baz as indented
        assert_eq!(scrips.len(), 1);
        let (root_line, indented) = as_indental(&scrips[0]).expect("should be indental");

        assert_eq!(root_line.len(), 1);
        assert_eq!(as_token(&root_line[0]), Some("foo"));

        assert_eq!(indented.len(), 2);
        assert_eq!(as_token(&indented[0]), Some("bar"));
        assert_eq!(as_token(&indented[1]), Some("baz"));
    }

    #[test]
    fn test_tokenize_same_indent_inside_parens() {
        let ops = make_operator_table();
        // Items at the same indent level inside invocation are flat
        let scrips = sequence("f(\n  a\n  b\n)", &ops).expect("should tokenize");

        // f(...) is one invocation with caller=f
        assert_eq!(scrips.len(), 1);

        let (_, caller, content) = as_invocation(&scrips[0]);
        assert_eq!(as_token(caller), Some("f"));
        // a and b at same indent level should be flat
        assert_eq!(content.len(), 2);
        assert_eq!(as_token(&content[0]), Some("a"));
        assert_eq!(as_token(&content[1]), Some("b"));
    }

    #[test]
    fn test_tokenize_indent_works_inside_parens() {
        let ops = make_operator_table();
        // Indentation creates structure inside invocations too
        let scrips = sequence("f(\n  a\n    b\n)", &ops).expect("should tokenize");

        // f(...) is one invocation with caller=f
        assert_eq!(scrips.len(), 1);

        let (_, caller, content) = as_invocation(&scrips[0]);
        assert_eq!(as_token(caller), Some("f"));
        // a with indented b should form an Indental inside the invocation
        assert_eq!(content.len(), 1);
        let (root_line, indented) = as_indental(&content[0]).expect("should be indental");
        assert_eq!(as_token(&root_line[0]), Some("a"));
        assert_eq!(as_token(&indented[0]), Some("b"));
    }

    #[test]
    fn test_tokenize_nested_indentation() {
        let ops = make_operator_table();
        let scrips = sequence("a\n  b\n    c\n    d\n  e", &ops).expect("should tokenize");

        // Structure: a -> [b -> [c, d], e]
        assert_eq!(scrips.len(), 1);
        let (root_line, indented) = as_indental(&scrips[0]).expect("should be indental");

        assert_eq!(root_line.len(), 1);
        assert_eq!(as_token(&root_line[0]), Some("a"));

        // indented should have: Indental(b -> [c, d]), e
        assert_eq!(indented.len(), 2);

        // First is nested indental
        let (inner_root, inner_indented) =
            as_indental(&indented[0]).expect("should be nested indental");
        assert_eq!(inner_root.len(), 1);
        assert_eq!(as_token(&inner_root[0]), Some("b"));
        assert_eq!(inner_indented.len(), 2);
        assert_eq!(as_token(&inner_indented[0]), Some("c"));
        assert_eq!(as_token(&inner_indented[1]), Some("d"));

        // Second is just e
        assert_eq!(as_token(&indented[1]), Some("e"));
    }

    #[test]
    fn test_tokenize_multiple_root_blocks() {
        let ops = make_operator_table();
        let scrips = sequence("a\n  b\nc\n  d", &ops).expect("should tokenize");

        // Two separate indental blocks at root level
        assert_eq!(scrips.len(), 2);

        let (root1, ind1) = as_indental(&scrips[0]).expect("first indental");
        assert_eq!(as_token(&root1[0]), Some("a"));
        assert_eq!(as_token(&ind1[0]), Some("b"));

        let (root2, ind2) = as_indental(&scrips[1]).expect("second indental");
        assert_eq!(as_token(&root2[0]), Some("c"));
        assert_eq!(as_token(&ind2[0]), Some("d"));
    }

    #[test]
    fn test_tokenize_indent_with_empty_lines() {
        let ops = make_operator_table();
        // Empty lines should be skipped
        let scrips = sequence("a\n  b\n\n  c", &ops).expect("should tokenize");

        assert_eq!(scrips.len(), 1);
        let (root_line, indented) = as_indental(&scrips[0]).expect("should be indental");

        assert_eq!(as_token(&root_line[0]), Some("a"));
        assert_eq!(indented.len(), 2);
        assert_eq!(as_token(&indented[0]), Some("b"));
        assert_eq!(as_token(&indented[1]), Some("c"));
    }

    #[test]
    fn test_tokenize_indent_with_operators() {
        let ops = make_operator_table();
        let scrips = sequence("x = y\n  a + b", &ops).expect("should tokenize");

        assert_eq!(scrips.len(), 1);
        let (root_line, indented) = as_indental(&scrips[0]).expect("should be indental");

        // Root line: x, =, y
        assert_eq!(root_line.len(), 3);
        assert_eq!(as_token(&root_line[0]), Some("x"));
        assert_eq!(as_token(&root_line[1]), Some("="));
        assert_eq!(as_token(&root_line[2]), Some("y"));

        // Indented: a, +, b
        assert_eq!(indented.len(), 3);
        assert_eq!(as_token(&indented[0]), Some("a"));
        assert_eq!(as_token(&indented[1]), Some("+"));
        assert_eq!(as_token(&indented[2]), Some("b"));
    }

    #[test]
    fn test_tokenize_no_indent_flat() {
        let ops = make_operator_table();
        // All at same indent level - no Indental structure
        let scrips = sequence("a\nb\nc", &ops).expect("should tokenize");

        // Should be 3 separate tokens, no indentals
        assert_eq!(scrips.len(), 3);
        assert_eq!(as_token(&scrips[0]), Some("a"));
        assert_eq!(as_token(&scrips[1]), Some("b"));
        assert_eq!(as_token(&scrips[2]), Some("c"));
    }

    #[test]
    fn test_tokenize_indent_dedent_indent() {
        let ops = make_operator_table();
        // a with indent, back to root, b with indent
        let scrips = sequence("a\n  x\nb\n  y", &ops).expect("should tokenize");

        assert_eq!(scrips.len(), 2);

        let (root1, ind1) = as_indental(&scrips[0]).expect("first indental");
        assert_eq!(as_token(&root1[0]), Some("a"));
        assert_eq!(as_token(&ind1[0]), Some("x"));

        let (root2, ind2) = as_indental(&scrips[1]).expect("second indental");
        assert_eq!(as_token(&root2[0]), Some("b"));
        assert_eq!(as_token(&ind2[0]), Some("y"));
    }

    #[test]
    fn test_tokenize_tab_indentation() {
        let ops = make_operator_table();
        let scrips = sequence("a\n\tb", &ops).expect("should tokenize");

        assert_eq!(scrips.len(), 1);
        let (root_line, indented) = as_indental(&scrips[0]).expect("should be indental");

        assert_eq!(as_token(&root_line[0]), Some("a"));
        assert_eq!(as_token(&indented[0]), Some("b"));
    }

    #[test]
    fn test_tokenize_indent_after_paren_closes() {
        let ops = make_operator_table();
        // Indent structure resumes after invocation closes
        let scrips = sequence("a\n  f(x)\n  b", &ops).expect("should tokenize");

        assert_eq!(scrips.len(), 1);
        let (root_line, indented) = as_indental(&scrips[0]).expect("should be indental");

        assert_eq!(as_token(&root_line[0]), Some("a"));

        // indented has: f(x) as one invocation, then b
        assert_eq!(indented.len(), 2);
        let (_, caller, content) = as_invocation(&indented[0]);
        assert_eq!(as_token(caller), Some("f"));
        assert_eq!(as_token(&content[0]), Some("x"));
        assert_eq!(as_token(&indented[1]), Some("b"));
    }

    #[test]
    fn test_tokenize_deeply_nested_indent() {
        let ops = make_operator_table();
        let scripsr = sequence("a\n  b\n    c\n      d", &ops);
        println!("{:?}", scripsr);
        let scrips = scripsr.expect("should tokenize");

        // a -> [b -> [c -> [d]]]
        assert_eq!(scrips.len(), 1);
        let (r1, i1) = as_indental(&scrips[0]).expect("level 1");
        assert_eq!(as_token(&r1[0]), Some("a"));

        let (r2, i2) = as_indental(&i1[0]).expect("level 2");
        assert_eq!(as_token(&r2[0]), Some("b"));

        let (r3, i3) = as_indental(&i2[0]).expect("level 3");
        assert_eq!(as_token(&r3[0]), Some("c"));

        assert_eq!(as_token(&i3[0]), Some("d"));
    }

    #[test]
    fn test_tokenize_inconsistent_indent_error() {
        let ops = make_operator_table();
        // First indent is 2 spaces, then 1 tab - neither is prefix of the other
        let result = sequence("a\n  b\n\tc", &ops);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.iter()
                .any(|e| e.message.contains("Inconsistent indentation"))
        );
    }

    #[test]
    fn test_tokenize_indent_discontinuity_in_parens() {
        let ops = make_operator_table();
        // Outside uses 2 spaces, inside invocation uses tabs - allowed because invocation resets indent
        let scrips = sequence("a\n  b(\n\tx\n\t\ty\n  )\n  c", &ops).expect("should tokenize");

        // Structure: a -> [b(...), c] where b(...) has caller=b and inside is x -> [y]
        assert_eq!(scrips.len(), 1);
        let (root, indented) = as_indental(&scrips[0]).expect("outer indental");
        assert_eq!(as_token(&root[0]), Some("a"));

        // indented has: b(...) as one invocation, then c
        assert_eq!(indented.len(), 2);

        let (_, caller, paren_content) = as_invocation(&indented[0]);
        assert_eq!(as_token(caller), Some("b"));
        // Inside invocation: x -> [y] using tab indentation
        assert_eq!(paren_content.len(), 1);
        let (inner_root, inner_indented) = as_indental(&paren_content[0]).expect("inner indental");
        assert_eq!(as_token(&inner_root[0]), Some("x"));
        assert_eq!(as_token(&inner_indented[0]), Some("y"));

        assert_eq!(as_token(&indented[1]), Some("c"));
    }

    #[test]
    fn test_tokenize_inconsistent_indent_error_inside_parens() {
        let ops = make_operator_table();
        // Inside parens, inconsistent indentation is still an error
        // First indent is 2 spaces, then 1 tab - neither is prefix of the other
        let result = sequence("f(\n  a\n\tb\n)", &ops);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.iter()
                .any(|e| e.message.contains("Inconsistent indentation"))
        );
    }

    #[test]
    fn test_tokenize_prefix_indent_works() {
        let ops = make_operator_table();
        // First level is 2 spaces, second level is 4 spaces (extends the first)
        let scrips = sequence("a\n  b\n    c", &ops).expect("should tokenize");
        println!("{:?}", scrips);
        assert_eq!(scrips.len(), 1);
        let (r1, i1) = as_indental(&scrips[0]).expect("level 1");
        assert_eq!(as_token(&r1[0]), Some("a"));

        let (r2, i2) = as_indental(&i1[0]).expect("level 2");
        assert_eq!(as_token(&r2[0]), Some("b"));
        assert_eq!(as_token(&i2[0]), Some("c"));
    }

    #[test]
    fn test_tokenize_mixed_space_tab_prefix() {
        let ops = make_operator_table();
        // First level is 2 spaces, second level is "  \t" (2 spaces + tab, extends the first)
        let scrips = sequence("a\n  b\n  \tc", &ops).expect("should tokenize");

        assert_eq!(scrips.len(), 1);
        let (r1, i1) = as_indental(&scrips[0]).expect("level 1");
        assert_eq!(as_token(&r1[0]), Some("a"));

        let (r2, i2) = as_indental(&i1[0]).expect("level 2");
        assert_eq!(as_token(&r2[0]), Some("b"));
        assert_eq!(as_token(&i2[0]), Some("c"));
    }

    #[test]
    fn test_tokenize_multi_level_dedent() {
        let ops = make_operator_table();
        // a at root, b indented, c more indented, d back at root
        // Should produce: Indental{a, [Indental{b, [c]}]}, d
        let scrips = sequence("a\n  b\n    c\nd", &ops).expect("should tokenize");

        assert_eq!(scrips.len(), 2);

        // First is the indental structure
        let (r1, i1) = as_indental(&scrips[0]).expect("outer indental");
        assert_eq!(as_token(&r1[0]), Some("a"));

        // Inside a's indented: b with c indented under it
        assert_eq!(i1.len(), 1);
        let (r2, i2) = as_indental(&i1[0]).expect("inner indental");
        assert_eq!(as_token(&r2[0]), Some("b"));
        assert_eq!(as_token(&i2[0]), Some("c"));

        // Second is d at root level
        assert_eq!(as_token(&scrips[1]), Some("d"));
    }

    #[test]
    fn test_tokenize_multi_sibling_indent_and_compare_ast() {
        let ops = make_operator_table();
        // multiline: a
        //   b
        //   c
        //   b
        //   c
        //
        // Should construct: Indental{a, [b, c, b, c]}
        let tokks = sequence("a\n  b\n   c\n  b\n   c", &ops).expect("should tokenize");

        assert_eq!(tokks.len(), 1);

        // The produced structure should be:
        // TokkV::Indental { root_line: [a], indented: [b, c, b, c] }
        use TokkV::*;

        let expected = make_indented(
            vec![make_token("a")],
            vec![
                make_indented(vec![make_token("b")], vec![make_token("c")]),
                make_indented(vec![make_token("b")], vec![make_token("c")]),
            ],
        );

        assert_eq!(tokks[0], expected, "AST structure does not match");
    }

    #[test]
    fn test_tokenize_paren_without_caller_error_inside_paren() {
        let ops = make_operator_table();
        // Inside a paren, another paren without a caller is an error
        let result = sequence("f(\n  (x)\n)", &ops);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.iter().any(|e| e.message.contains("requires a caller")));
    }

    #[test]
    fn test_tokenize_paren_after_indent_steals_caller() {
        let ops = make_operator_table();
        // Paren immediately after indent increase steals caller from parent
        // foo\n  (bar) becomes foo(bar), not Indental{foo, [(bar)]}
        let scrips = sequence("foo\n  (bar)", &ops).expect("should tokenize");

        // Should be one invocation with caller=foo
        assert_eq!(scrips.len(), 1);
        let (_, caller, content) = as_invocation(&scrips[0]);
        assert_eq!(as_token(caller), Some("foo"));
        assert_eq!(content.len(), 1);
        assert_eq!(as_token(&content[0]), Some("bar"));
    }

    #[test]
    fn test_tokenize_paren_after_indent_with_multiple_args() {
        let ops = make_operator_table();
        // foo\n  (a b c) becomes foo(a b c)
        let scrips = sequence("foo\n  (a b c)", &ops).expect("should tokenize");

        assert_eq!(scrips.len(), 1);
        let (_, caller, content) = as_invocation(&scrips[0]);
        assert_eq!(as_token(caller), Some("foo"));
        assert_eq!(content.len(), 3);
        assert_eq!(as_token(&content[0]), Some("a"));
        assert_eq!(as_token(&content[1]), Some("b"));
        assert_eq!(as_token(&content[2]), Some("c"));
    }

    #[test]
    fn test_tokenize_nested_paren_needs_caller() {
        let ops = make_operator_table();
        // g(x) inside f() is OK because g is the caller
        let scrips = sequence("f(g(x))", &ops).expect("should tokenize");
        assert_eq!(scrips.len(), 1);

        // But ((x)) should fail because inner ( has no caller
        let result = sequence("f((x))", &ops);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.iter().any(|e| e.message.contains("requires a caller")));
    }
}
