// overview: This is a very flexible parser for languages that may or may not use indentation to structure, which have a fixed number of infix operators, with precedence. This was entirely sufficient to implement the quite sophisticated syntax of the ~~Bjork~~Kaba language.
// the tokenizer converts a string into Scrips, which are sometimes like tokens, other times richer and more structured, as they can retain indent structure. The Scrips are then transformed through term rewriting into Asts.
// keywords: fn, to, if, else, elif
// A decision was made to have conditional and function bodies be lists of statements, instead of single expressions, as although a single expression can be a block expression, the user generally isn't aware of the distinction between non-block and block expressions that contain a single non-block expression, it wont be rendered in the ui, and they wont be able to control it, so it doesn't make sense to have that distinction in the Code type.

// I'm considering deleting the +(a b c) syntax. It's confusing, it's a violation of how inline syntax usually works, and it doesn't make a huge amount of sense for operators to be variadic.
// But ultimately I think there's no way to reconcile parens for order of operation control with having no commas. And of course a lack of commas also makes arg lists messier sometimes

use std::mem::{replace, take};
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

fn $x.isOdd()

fn a b=to $x + 2 to $x.isOdd()


b = fn x = to +
    $x
    2
× indental function definition is missing its body. The indental is being taken by '+'.
    hint:
b = fn x = to $x + 2
    # [todo]: return something

# should this be illegal:
a
+2
# and
f(2 3)
# I think so. I think the meaning of a line should be clear just from this line. Lists and

# also troubled by the fact that this works
f 2
    3
# but this doesn't
f 2 3
# I actually really like inline...

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuoteType {
    Single,   // '
    Double,   // "
    Backtick, // `
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
    caller: Option<Box<Toast>>,
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

    fn wrap_current_line(&mut self) {
        // Wrap the current line (from line_start onwards) in a Line toast
        let top = self.indent_stack.last_mut().unwrap();
        if top.line_start < top.content.len() {
            let line_items: Vec<_> = top.content.drain(top.line_start..).collect();
            if !line_items.is_empty() {
                let span = Span::from_range(
                    line_items.first().unwrap().span().start,
                    line_items.last().unwrap().span().end(),
                );
                top.content.push(Toast {
                    span,
                    v: ToastV::Line(line_items),
                });
                // Update line_start so we don't double-wrap if called again
                top.line_start = top.content.len();
            }
        }
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
            // root_line is NOT wrapped in Line - it stays as Vec<Toast>
            let span = {
                let start = root_line.first().unwrap().span().start;
                let end = if let Some(t) = indented.last() {
                    t.span().end()
                } else {
                    root_line.last().unwrap().span().end()
                };
                Span::from_range(start, end)
            };

            host.content.push(Toast {
                span,
                v: ToastV::Indental {
                    root_line,
                    indented,
                },
            });
        }

        self.known_indent.truncate(host.indent_len);
    }

    /// Flush all indent levels, collapsing them into content
    /// called when the end of a paren level is reached
    fn pop_all_indents(&mut self, _end_pos: usize) {
        while self.indent_stack.len() > 1 {
            self.wrap_current_line(); // Wrap the line before outdenting
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

/// Comma is a special separator character (not an operator, not a regular token)
const COMMA: char = ',';

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
// [todo] use a hashmap with an initial hash function of prime modulus then switch to a real one on collision
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

/// Create a Toast containing an Atom with the given value
pub fn make_atom(span: Span, value: String) -> Toast {
    Toast {
        span,
        v: ToastV::Atom { value },
    }
}

pub fn sequence(source: &str, operators: &OperatorTable) -> Result<Vec<Toast>, Vec<Error>> {
    use std::iter::Peekable;
    use std::str::Chars;

    let mut pos: usize = 0;
    let mut chars: Peekable<Chars> = source.chars().peekable();

    let mut errors: Vec<Error> = Vec::new();

    // Stack of open invocations - bottom entry is never popped
    let mut invocation_stack: Vec<InvocationLevel> = vec![InvocationLevel {
        caller: Some(Box::new(make_atom(Span::new(0, 0), "do".to_string()))),
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
                // Bracket line handling:
                // - At same/shallower indent: wrap previous line to prevent stealing
                // - At deeper indent: don't wrap yet, create indent level first
                //   so the bracket becomes part of the indental structure
                if new_len <= base_len {
                    current.wrap_current_line();
                    let top = current.indent_stack.last_mut().unwrap();
                    top.line_start = top.content.len();
                } else {
                    // Deeper indent - create the indent level so this becomes an indental
                    // Push a new indent level for the bracket
                    current.indent_stack.push(IndentLevel {
                        indent_len: new_len,
                        line_start: 0,
                        content: Vec::new(),
                        span_start: pos,
                    });
                }
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

                if !is_recognized_prefix {
                    errors.push(Error::new(
                        Span::new(indent_start, new_len.max(1)),
                        "Inconsistent indentation: whitespace pattern doesn't match previous levels",
                    ));
                }

                // While new len is less than previous indent levels, pop them
                while let Some(top) = current.indent_stack.last()
                    && new_len < top.indent_len
                {
                    current.wrap_current_line(); // Wrap the line before outdenting
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
                    // Staying at same level - wrap the previous line and start a new one
                    current.wrap_current_line();
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

            // Comma - separator that prevents stealing
            ',' => {
                // Wrap the current line to prevent the next item from stealing
                let current = invocation_stack.last_mut().unwrap();
                current.wrap_current_line();
                let top = current.indent_stack.last_mut().unwrap();
                top.line_start = top.content.len();
                chars.next();
                pos += 1;
            }

            // Opening brackets - start an invocation
            '(' | '[' | '{' => {
                let paren_type = ParenType::from_open(c);

                // Try to steal the previous token as the caller
                // Don't steal if it's a Line (represents a completed previous line) or an Operator
                // If there's nothing to steal, this is a None invocation
                let caller: Option<Box<Toast>> = {
                    let list = end_list(&mut invocation_stack);
                    if let Some(last) = list.last() {
                        if matches!(&last.v, ToastV::Line(_) | ToastV::Operator(_)) {
                            None // Don't steal completed lines or operators
                        } else {
                            list.pop().map(Box::new)
                        }
                    } else {
                        None
                    }
                };

                // Adjust span_start to include the caller if present, otherwise use current pos
                let span_start = caller.as_ref().map_or(pos, |c| c.span().start);

                invocation_stack.push(InvocationLevel {
                    paren_type,
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
                    span_start,
                    ..
                } = invocation_level;
                let parameters = indent_stack.drain(..).next().unwrap().content;

                // Create either Paren or Invocation based on whether there's a caller
                let ast = Toast {
                    span: Span::from_range(span_start, pos + 1),
                    v: if let Some(caller_box) = caller {
                        ToastV::Invocation {
                            caller: caller_box,
                            kind: paren_type,
                            parameters,
                        }
                    } else {
                        ToastV::Paren(parameters)
                    },
                };

                // Add to parent's output
                let parent = invocation_stack.last_mut().unwrap();
                parent.end_list().push(ast);

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

                let span = Span::from_range(start, pos);
                let ast = Toast {
                    span,
                    v: if quote_type == QuoteType::Backtick {
                        ToastV::Atom { value }
                    } else {
                        ToastV::Quoted { quote_type, value }
                    },
                };
                end_list(&mut invocation_stack).push(ast);
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

                let toast = Toast {
                    span: Span::from_range(start, pos),
                    v: ToastV::Operator(op),
                };
                let current = invocation_stack.last_mut().unwrap();
                current.end_list().push(toast);
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
                    let comment = Toast {
                        span,
                        v: ToastV::Comment { content },
                    };
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
                    let comment = Toast {
                        span,
                        v: ToastV::Comment { content },
                    };
                    end_list(&mut invocation_stack).push(comment);
                }
            }

            // Regular token (identifier, number, etc.)
            _ => {
                let start = pos;
                let mut value = String::new();

                while let Some(&ch) = chars.peek() {
                    // Stop at whitespace, brackets, operators, quotes, commas, or comments
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
                        || ch == ','
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
        root.wrap_current_line(); // Wrap the final line
        root.pop_all_indents(pos); // This will wrap and pop nested indents
        // Return the content of the base indent level
        Ok(root.indent_stack.drain(..).next().unwrap().content)
    }
}

/// Conditional AST node data (boxed in ToastV::Conditional to keep enum size small)
#[derive(Debug, PartialEq, Clone)]
pub struct Conditional {
    pub condition: Toast,
    pub then: Vec<Toast>,
    pub elsifs: Vec<(Toast, Vec<Toast>)>,
    pub elsen: Vec<Toast>,
}

// ============================================================================
// Pure AST Types - Only contain final parsed nodes
// ============================================================================

/// Pure AST node - guaranteed to contain only final parsed structures
#[derive(Debug, Clone)]
pub struct Ast {
    pub span: Span,
    pub v: AstV,
}

/// Pure AST variants - no intermediate parsing states
#[derive(Debug, Clone)]
pub enum AstV {
    Invocation {
        caller: Box<Ast>,
        kind: ParenType,
        parameters: Vec<Ast>,
    },
    Comment {
        content: String,
    },
    Conditional {
        condition: Box<Ast>,
        then: Vec<Ast>,
        elsifs: Vec<(Ast, Vec<Ast>)>,
        elsen: Vec<Ast>,
    },
    Function {
        args: Vec<Ast>,
        return_type: Option<Box<Ast>>,
        body: Vec<Ast>,
    },
    Totion {
        return_type: Option<Box<Ast>>,
        body: Vec<Ast>,
    },
    Block {
        statements: Vec<Ast>,
    },
    Atom {
        value: String,
    },
    Quoted {
        quote_type: QuoteType,
        value: String,
    },
}

impl PartialEq for Ast {
    fn eq(&self, other: &Self) -> bool {
        self.v == other.v
    }
}

impl PartialEq for AstV {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                AstV::Invocation {
                    caller: h1,
                    kind: k1,
                    parameters: p1,
                },
                AstV::Invocation {
                    caller: h2,
                    kind: k2,
                    parameters: p2,
                },
            ) => h1 == h2 && k1 == k2 && p1 == p2,
            (AstV::Comment { content: c1 }, AstV::Comment { content: c2 }) => c1 == c2,
            (
                AstV::Conditional {
                    condition: c1,
                    then: t1,
                    elsifs: ei1,
                    elsen: en1,
                },
                AstV::Conditional {
                    condition: c2,
                    then: t2,
                    elsifs: ei2,
                    elsen: en2,
                },
            ) => c1 == c2 && t1 == t2 && ei1 == ei2 && en1 == en2,
            (
                AstV::Function {
                    args: a1,
                    return_type: r1,
                    body: b1,
                },
                AstV::Function {
                    args: a2,
                    return_type: r2,
                    body: b2,
                },
            ) => a1 == a2 && r1 == r2 && b1 == b2,
            (
                AstV::Totion {
                    return_type: r1,
                    body: b1,
                },
                AstV::Totion {
                    return_type: r2,
                    body: b2,
                },
            ) => r1 == r2 && b1 == b2,
            (AstV::Block { statements: s1 }, AstV::Block { statements: s2 }) => s1 == s2,
            (AstV::Atom { value: v1 }, AstV::Atom { value: v2 }) => v1 == v2,
            (
                AstV::Quoted {
                    quote_type: q1,
                    value: v1,
                },
                AstV::Quoted {
                    quote_type: q2,
                    value: v2,
                },
            ) => q1 == q2 && v1 == v2,
            _ => false,
        }
    }
}

/// The unified Toast type - combines what was previously Tokk and ToastAst
#[derive(Debug, Clone)]
pub struct Toast {
    pub span: Span,
    pub v: ToastV,
}

#[derive(Debug, Clone)]
pub enum ToastV {
    // "Tokk" variants (intermediate parsing state)
    Operator(String),
    Indental {
        root_line: Vec<Toast>,
        indented: Vec<Toast>,
    },
    /// things self-replace with error when they had an irrecoverable error during structuring
    Error,
    /// Used as placeholder during tree manipulation
    Dummy,
    /// a line (that isn't a root line)
    Line(Vec<Toast>),
    /// Parenthesized expression without a caller (not an ast_type)
    Paren(Vec<Toast>),

    // "Ast" variants (final parsed state)
    Invocation {
        caller: Box<Toast>,
        kind: ParenType,
        parameters: Vec<Toast>,
    },
    Comment {
        content: String,
    },
    Conditional(Box<Conditional>),
    Function {
        args: Vec<Toast>,
        return_type: Option<Box<Toast>>,
        body: Vec<Toast>,
    },
    /// A standalone `to` expression — like an anonymous body / thunk.
    /// Only produced when `to` isn't consumed by a `fn` scope or operator.
    Totion {
        return_type: Option<Box<Toast>>,
        body: Vec<Toast>,
    },
    Block {
        statements: Vec<Toast>,
    },
    Atom {
        value: String,
    },
    Quoted {
        quote_type: QuoteType,
        value: String,
    },
}

/// Equality comparison for Toast ignoring span
impl PartialEq for Toast {
    fn eq(&self, other: &Self) -> bool {
        self.v == other.v
    }
}

/// Equality comparison for ToastV ignoring spans
impl PartialEq for ToastV {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ToastV::Operator(a), ToastV::Operator(b)) => a == b,
            (
                ToastV::Indental {
                    root_line: r1,
                    indented: i1,
                },
                ToastV::Indental {
                    root_line: r2,
                    indented: i2,
                },
            ) => r1 == r2 && i1 == i2,
            (ToastV::Dummy, ToastV::Dummy) => true,
            (ToastV::Error, ToastV::Error) => true,
            (ToastV::Paren(p1), ToastV::Paren(p2)) => p1 == p2,
            (
                ToastV::Invocation {
                    caller: h1,
                    kind: k1,
                    parameters: p1,
                },
                ToastV::Invocation {
                    caller: h2,
                    kind: k2,
                    parameters: p2,
                },
            ) => h1 == h2 && k1 == k2 && p1 == p2,
            (ToastV::Comment { content: c1 }, ToastV::Comment { content: c2 }) => c1 == c2,
            (ToastV::Conditional(c1), ToastV::Conditional(c2)) => c1 == c2,
            (
                ToastV::Function {
                    args: a1,
                    return_type: r1,
                    body: b1,
                },
                ToastV::Function {
                    args: a2,
                    return_type: r2,
                    body: b2,
                },
            ) => a1 == a2 && r1 == r2 && b1 == b2,
            (
                ToastV::Totion {
                    return_type: r1,
                    body: b1,
                },
                ToastV::Totion {
                    return_type: r2,
                    body: b2,
                },
            ) => r1 == r2 && b1 == b2,
            (ToastV::Block { statements: s1 }, ToastV::Block { statements: s2 }) => s1 == s2,
            (ToastV::Atom { value: v1 }, ToastV::Atom { value: v2 }) => v1 == v2,
            (
                ToastV::Quoted {
                    quote_type: q1,
                    value: v1,
                },
                ToastV::Quoted {
                    quote_type: q2,
                    value: v2,
                },
            ) => q1 == q2 && v1 == v2,
            _ => false,
        }
    }
}

impl Toast {
    /// Returns true if this is a final AST variant (not Operator, Indental, Paren, or Dummy)
    pub fn is_ast(&self) -> bool {
        !matches!(
            self.v,
            ToastV::Operator(_)
                | ToastV::Indental { .. }
                | ToastV::Dummy
                | ToastV::Error
                | ToastV::Paren(_)
        )
    }

    pub fn span(&self) -> Span {
        self.span
    }

    /// Makes sure every toast is an ast
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
        if !self.is_ast() {
            errors.push(Error::new(self.span, "Toast is not an Ast"));
            return;
        }
        match &self.v {
            ToastV::Invocation {
                caller, parameters, ..
            } => {
                caller.verify_ast_writer(errors);
                for p in parameters {
                    p.verify_ast_writer(errors);
                }
            }
            ToastV::Conditional(cond) => {
                cond.condition.verify_ast_writer(errors);
                for t in &cond.then {
                    t.verify_ast_writer(errors);
                }
                for e in &cond.elsen {
                    e.verify_ast_writer(errors);
                }
                for (c, b) in &cond.elsifs {
                    c.verify_ast_writer(errors);
                    for stmt in b {
                        stmt.verify_ast_writer(errors);
                    }
                }
            }
            ToastV::Function {
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
                for stmt in body {
                    stmt.verify_ast_writer(errors);
                }
            }
            ToastV::Totion { return_type, body } => {
                if let Some(r) = return_type {
                    r.verify_ast_writer(errors);
                }
                for stmt in body {
                    stmt.verify_ast_writer(errors);
                }
            }
            ToastV::Block { statements, .. } => {
                for st in statements {
                    st.verify_ast_writer(errors);
                }
            }
            // Leaf nodes - no children to recurse into
            ToastV::Comment { .. } | ToastV::Atom { .. } | ToastV::Quoted { .. } => {}
            // Non-AST variants already handled above
            ToastV::Operator(_)
            | ToastV::Indental { .. }
            | ToastV::Line(_)
            | ToastV::Paren(_)
            | ToastV::Dummy
            | ToastV::Error => {}
        }
    }

    /// Convert Toast to pure Ast type. Returns error if Toast contains non-AST variants.
    pub fn to_ast(self) -> Result<Ast, Error> {
        if !self.is_ast() {
            return Err(Error::new(self.span, "Toast is not a valid Ast node"));
        }

        let span = self.span;
        let v = match self.v {
            ToastV::Invocation {
                caller,
                kind,
                parameters,
            } => {
                let caller = Box::new((*caller).to_ast()?);
                let parameters = parameters
                    .into_iter()
                    .map(|p| p.to_ast())
                    .collect::<Result<Vec<_>, _>>()?;
                AstV::Invocation {
                    caller,
                    kind,
                    parameters,
                }
            }
            ToastV::Comment { content } => AstV::Comment { content },
            ToastV::Conditional(cond) => {
                let condition = Box::new(cond.condition.to_ast()?);
                let then = cond
                    .then
                    .into_iter()
                    .map(|t| t.to_ast())
                    .collect::<Result<Vec<_>, _>>()?;
                let elsifs = cond
                    .elsifs
                    .into_iter()
                    .map(|(c, b)| {
                        let cond = c.to_ast()?;
                        let body = b
                            .into_iter()
                            .map(|s| s.to_ast())
                            .collect::<Result<Vec<_>, _>>()?;
                        Ok((cond, body))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let elsen = cond
                    .elsen
                    .into_iter()
                    .map(|e| e.to_ast())
                    .collect::<Result<Vec<_>, _>>()?;
                AstV::Conditional {
                    condition,
                    then,
                    elsifs,
                    elsen,
                }
            }
            ToastV::Function {
                args,
                return_type,
                body,
            } => {
                let args = args
                    .into_iter()
                    .map(|a| a.to_ast())
                    .collect::<Result<Vec<_>, _>>()?;
                let return_type = return_type
                    .map(|r| (*r).to_ast().map(Box::new))
                    .transpose()?;
                let body = body
                    .into_iter()
                    .map(|s| s.to_ast())
                    .collect::<Result<Vec<_>, _>>()?;
                AstV::Function {
                    args,
                    return_type,
                    body,
                }
            }
            ToastV::Totion { return_type, body } => {
                let return_type = return_type
                    .map(|r| (*r).to_ast().map(Box::new))
                    .transpose()?;
                let body = body
                    .into_iter()
                    .map(|s| s.to_ast())
                    .collect::<Result<Vec<_>, _>>()?;
                AstV::Totion { return_type, body }
            }
            ToastV::Block { statements } => {
                let statements = statements
                    .into_iter()
                    .map(|s| s.to_ast())
                    .collect::<Result<Vec<_>, _>>()?;
                AstV::Block { statements }
            }
            ToastV::Atom { value } => AstV::Atom { value },
            ToastV::Quoted { quote_type, value } => AstV::Quoted { quote_type, value },
            // Non-AST variants
            ToastV::Operator(_)
            | ToastV::Indental { .. }
            | ToastV::Line(_)
            | ToastV::Paren(_)
            | ToastV::Dummy
            | ToastV::Error => {
                return Err(Error::new(span, "Toast contains non-AST variant"));
            }
        };

        Ok(Ast { span, v })
    }
}

/**
the astrule step takes a sequence of [Toast]s that are initially all [Tokk]s and applies some rewrite rules to transform them into [Ast] Toasts, which are then stripped down into a tree of pure Asts.
`%endfirst` means it's greedy but from the other direction, processing terms from the right first (this may be what lazy means in general for all I know, but I think endfirst is a much clearer term for this, it means it'll be intuitive if we also use this for right-associativity)

# backtick quotes just translate to an atom, ie, we use them to allow expressing symbols with spaces in them.
for $x:Quoted(Backticked) → ast::Atom($x)

# operators can join lines
%Line($x* $o:operator) %Line($y*) → %Line($x* $o $y*)

# lines with multiple root entries are invocations
%Line($x $y+) → $x($y+)*

def %indenter = fn | if | do | to

%indental($o:operators $x*)($y*) → $o($x $y)

# todo: inline syntaxes like to $x (not sure how they interact with pratt parsing/operators as rules)

# the final open indenter is what takes the indental body
%indental(%lazy($x* $i:indenter $y*))(z*) → $x $indental($i $y)($z*)

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

struct Structurer {
    toasts: Vec<Toast>,
    operator_table: OperatorTable,
    errors: Vec<Error>,
}

impl Structurer {
    pub fn new(toasts: Vec<Toast>, operator_table: OperatorTable) -> Self {
        Self {
            toasts,
            operator_table,
            errors: Vec::new(),
        }
    }
    // true iff it did, in which case return $1
    fn consider_opening_sequence(toast: &Toast) -> (bool, Option<SequenceType>) {
        match &toast.v {
            ToastV::Atom { value } => (true, SequenceType::from_label(value)),
            _ => (false, None),
        }
    }
    pub fn structure(mut self) -> Result<Vec<Toast>, Vec<Error>> {
        let toasts = take(&mut self.toasts);
        let mut toasts = toasts;
        self.structure_series(&mut toasts, None);
        self.toasts = toasts;
        let Self {
            errors, mut toasts, ..
        } = self;

        // If there's a single root "do" invocation, unwrap it
        if toasts.len() == 1 {
            if let Some(Toast {
                v: ToastV::Invocation {
                    caller, parameters, ..
                },
                ..
            }) = toasts.first()
            {
                if let ToastV::Atom { value } = &caller.v {
                    if value == "do" {
                        toasts = parameters.clone();
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(toasts)
        } else {
            Err(errors)
        }
    }

    fn structure_series(
        &mut self,
        toasts: &mut Vec<Toast>,
        indented: Option<Vec<Toast>>,
    ) -> Option<SequenceType> {
        // first apply the operator line joining rule
        
        
        for toast in toasts.iter_mut() {
            self.structure_individual(toast);
        }

        // Inline pratt_parse_series logic
        let mut items: Vec<Toast> = toasts.drain(..).collect();
        let mut pos: usize = 0;

        while pos < items.len() {
            match &items[pos].v {
                ToastV::Comment { .. } => {
                    toasts.push(replace(&mut items[pos], dummy_toast()));
                    pos += 1;
                }
                ToastV::Dummy => {
                    pos += 1;
                }
                ToastV::Operator(_) => {
                    let t = replace(&mut items[pos], dummy_toast());
                    self.errors.push(Error::new(
                        t.span,
                        "unexpected operator in expression position",
                    ));
                    pos += 1;
                }
                _ => {
                    let expr = self.pratt_parse_bp_recurse(&mut items, &mut pos, 0);
                    toasts.push(expr);
                }
            }
        }

        // Collect if/elif/else chains at the series level
        self.collect_conditional_chains(toasts);

        // now prepare indental if needed
        if let Some(mut indented) = indented {
            self.structure_series(&mut indented, None);
            if let Some(assembled) = self.assemble_indental(toasts, indented) {
                toasts.push(assembled);
            }
        }

        None
    }

    fn pratt_parse_bp_recurse(
        &mut self,
        items: &mut [Toast],
        pos: &mut usize,
        min_bp: u8,
    ) -> Toast {
        let mut left = replace(&mut items[*pos], dummy_toast());
        *pos += 1;

        loop {
            if *pos >= items.len() {
                break;
            }

            let (op_str, bp) = match &items[*pos].v {
                ToastV::Operator(s) => {
                    if let Some(bp) = self.operator_table.lookup(s) {
                        (s.clone(), bp)
                    } else {
                        break;
                    }
                }
                _ => break,
            };

            if bp.left < min_bp {
                break;
            }

            let op_toast = replace(&mut items[*pos], dummy_toast());
            *pos += 1;

            if *pos >= items.len() {
                self.errors.push(Error::new(
                    op_toast.span,
                    format!("operator '{}' missing right operand", op_str),
                ));
                return left;
            }
            if matches!(
                &items[*pos].v,
                ToastV::Operator(_) | ToastV::Dummy | ToastV::Comment { .. }
            ) {
                self.errors.push(Error::new(
                    items[*pos].span,
                    format!("expected operand after '{}'", op_str),
                ));
                return left;
            }

            let right = self.pratt_parse_bp_recurse(items, pos, bp.right);

            // Convert operator to atom for use as invocation caller
            let op_atom = if let ToastV::Operator(op_str) = op_toast.v {
                Toast {
                    span: op_toast.span,
                    v: ToastV::Atom { value: op_str },
                }
            } else {
                op_toast
            };

            let span = Span::from_range(left.span.start, right.span.end());
            left = Toast {
                span,
                v: ToastV::Invocation {
                    caller: Box::new(op_atom),
                    kind: ParenType::Round,
                    parameters: vec![left, right],
                },
            };
        }

        left
    }

    fn structure_individual(&mut self, toast: &mut Toast) {
        let span = toast.span;
        match &mut toast.v {
            ToastV::Indental { .. } => {
                // Delegate entirely to structure_series with the indented content
                let new_v = match replace(&mut toast.v, ToastV::Dummy) {
                    ToastV::Indental {
                        mut root_line,
                        indented,
                    } => {
                        // structure_series handles: keyword scanning, indental host
                        // detection, assembly, and Pratt parsing — all in one call
                        self.structure_series(&mut root_line, Some(indented));

                        if root_line.is_empty() {
                            self.errors
                                .push(Error::new(span, "indental resolved to nothing"));
                            ToastV::Error
                        } else if root_line.len() == 1 {
                            root_line.remove(0).v
                        } else {
                            ToastV::Block {
                                statements: root_line,
                            }
                        }
                    }
                    _ => unreachable!(),
                };
                toast.v = new_v;
            }
            ToastV::Line(_) => {
                self.errors.push(Error::new(span, "no Line Toasts should remain at the time when structure_individual is called"));
            }
            ToastV::Dummy | ToastV::Error => {}
            ToastV::Paren(items) => {
                // Structure each item in the paren individually
                for item in items.iter_mut() {
                    self.structure_individual(item);
                }
            }
            ToastV::Invocation {
                caller: head,
                parameters,
                ..
            } => {
                // Structure the head
                self.structure_individual(head);

                // Check if head is a keyword needing special handling
                let keyword = if let ToastV::Atom { value } = &head.v {
                    SequenceType::from_label(value)
                } else {
                    None
                };

                match keyword {
                    Some(SequenceType::Function) => {
                        // Don't structure_series the params — we need to find `to` before Pratt parsing
                        for p in parameters.iter_mut() {
                            self.structure_individual(p);
                        }
                        let params = take(parameters);
                        let assembled = self.assemble_function(span, params, Vec::new());
                        toast.v = assembled.v;
                    }
                    Some(SequenceType::Totion) => {
                        // Paren form: to(body) or to(:type body)
                        for p in parameters.iter_mut() {
                            self.structure_individual(p);
                        }
                        let params = take(parameters);
                        let assembled = self.assemble_totion(span, params, Vec::new());
                        toast.v = assembled.v;
                    }
                    Some(SequenceType::Conditional) => {
                        for p in parameters.iter_mut() {
                            self.structure_individual(p);
                        }
                        let params = take(parameters);
                        let assembled = self.assemble_conditional(span, params, Vec::new());
                        toast.v = assembled.v;
                    }
                    Some(SequenceType::Elif) => {
                        for p in parameters.iter_mut() {
                            self.structure_individual(p);
                        }
                        let params = take(parameters);
                        let assembled = self.assemble_elif(span, params, Vec::new());
                        toast.v = assembled.v;
                    }
                    Some(SequenceType::Else) => {
                        self.structure_series(parameters, None);
                        let params = take(parameters);
                        let assembled = self.assemble_else(span, params);
                        toast.v = assembled.v;
                    }
                    Some(SequenceType::Block) => {
                        self.structure_series(parameters, None);
                        let statements = take(parameters);
                        toast.v = ToastV::Block { statements };
                    }
                    Some(SequenceType::LeadOperator) | Some(SequenceType::TailOperator) => {
                        // These aren't actually used in keyword position, treat as normal invocation
                        self.structure_series(parameters, None);
                    }
                    None => {
                        // Normal invocation
                        self.structure_series(parameters, None);
                    }
                }
            }
            ToastV::Conditional(cond) => {
                self.structure_individual(&mut cond.condition);
                self.structure_series(&mut cond.then, None);
                self.structure_series(&mut cond.elsen, None);
                for (c, b) in &mut cond.elsifs {
                    self.structure_individual(c);
                    self.structure_series(b, None);
                }
            }
            ToastV::Function {
                args,
                return_type,
                body,
                ..
            } => {
                self.structure_series(args, None);
                if let Some(ret) = return_type {
                    self.structure_individual(ret);
                }
                self.structure_series(body, None);
            }
            ToastV::Totion { return_type, body } => {
                if let Some(ret) = return_type {
                    self.structure_individual(ret);
                }
                self.structure_series(body, None);
            }
            ToastV::Block { statements, .. } => {
                self.structure_series(statements, None);
            }
            ToastV::Comment { .. }
            | ToastV::Atom { .. }
            | ToastV::Quoted { .. }
            | ToastV::Operator(_) => {}
        }
    }

    /// Parse expressions in the toast sequence.
    /// If `parse_one` is true, parse only the first expression and leave the rest in `toasts`.
    /// If false, parse all expressions in the sequence.
    fn pratt_parse_series(&mut self, toasts: &mut Vec<Toast>, parse_one: bool) {
        if toasts.is_empty() {
            return;
        }

        let mut items: Vec<Toast> = toasts.drain(..).collect();
        let mut pos: usize = 0;

        while pos < items.len() {
            match &items[pos].v {
                ToastV::Comment { .. } => {
                    self.errors.push(Error::new(
                        items[pos].span,
                        "comments should be removed before pratt parsing",
                    ));
                    pos += 1;
                }
                ToastV::Dummy => {
                    pos += 1;
                }
                ToastV::Operator(_) => {
                    let t = replace(&mut items[pos], dummy_toast());
                    self.errors.push(Error::new(
                        t.span,
                        "unexpected operator in expression position",
                    ));
                    pos += 1;
                }
                _ => {
                    let expr = self.pratt_parse_bp_recurse(&mut items, &mut pos, 0);
                    toasts.push(expr);

                    if parse_one {
                        // Put remaining items back into toasts
                        while pos < items.len() {
                            let item = replace(&mut items[pos], dummy_toast());
                            if !matches!(item.v, ToastV::Dummy) {
                                toasts.push(item);
                            }
                            pos += 1;
                        }
                        return;
                    }
                }
            }
        }
    }

    fn assemble_function(
        &mut self,
        span: Span,
        mut after_fn: Vec<Toast>,
        indented: Vec<Toast>,
    ) -> Toast {
        let to_pos = after_fn
            .iter()
            .position(|t| matches!(&t.v, ToastV::Atom { value } if value == "to"));

        let (args, return_type, body) = match to_pos {
            Some(tp) => {
                let mut after_to: Vec<Toast> = after_fn.drain(tp..).collect();
                after_to.remove(0); // consume `to` keyword
                let mut args = after_fn; // what remains before `to`

                // Check for optional `:` before return type
                let (rt, mut inline_body) = if !after_to.is_empty()
                    && matches!(&after_to[0].v, ToastV::Operator(s) if s == ":")
                {
                    after_to.remove(0); // skip `:`
                    self.pratt_parse_series(&mut after_to, true); // parse one: just the return type
                    if after_to.is_empty() {
                        (None, Vec::new())
                    } else {
                        let rt = after_to.remove(0);
                        (Some(Box::new(rt)), after_to)
                    }
                } else {
                    // No `:` — no explicit return type, everything after `to` is body
                    (None, after_to)
                };

                self.pratt_parse_series(&mut args, false);
                self.pratt_parse_series(&mut inline_body, false);
                inline_body.extend(indented);

                (args, rt, inline_body)
            }
            None => {
                // No `to`: everything is args, body comes from indented only
                self.pratt_parse_series(&mut after_fn, false);
                (after_fn, None, indented)
            }
        };

        Toast {
            span,
            v: ToastV::Function {
                args,
                return_type,
                body,
            },
        }
    }

    fn assemble_conditional(
        &mut self,
        span: Span,
        mut after_if: Vec<Toast>,
        indented: Vec<Toast>,
    ) -> Toast {
        if !indented.is_empty() {
            // Indental form: after_if = condition tokens, indented = then-body
            self.pratt_parse_series(&mut after_if, true); // parse one: just the condition
            let condition = if after_if.is_empty() {
                self.errors.push(Error::new(span, "if without condition"));
                Toast {
                    span,
                    v: ToastV::Error,
                }
            } else {
                if after_if.len() > 1 {
                    self.errors.push(Error::new(
                        span,
                        "unexpected tokens after if condition in indental form",
                    ));
                }
                after_if.remove(0)
            };
            Toast {
                span,
                v: ToastV::Conditional(Box::new(Conditional {
                    condition,
                    then: indented,
                    elsifs: Vec::new(),
                    elsen: Vec::new(),
                })),
            }
        } else {
            // Paren form: if(condition then... else elsen...)
            self.pratt_parse_series(&mut after_if, false);
            let else_pos = after_if
                .iter()
                .position(|t| matches!(&t.v, ToastV::Atom { value } if value == "else"));

            let (mut then_body, elsen) = match else_pos {
                Some(ep) => {
                    let mut elsen: Vec<Toast> = after_if.drain(ep..).collect();
                    elsen.remove(0); // remove `else` keyword
                    (after_if, elsen)
                }
                None => (after_if, Vec::new()),
            };

            if then_body.is_empty() {
                self.errors.push(Error::new(span, "if without condition"));
                return Toast {
                    span,
                    v: ToastV::Error,
                };
            }
            let condition = then_body.remove(0);

            Toast {
                span,
                v: ToastV::Conditional(Box::new(Conditional {
                    condition,
                    then: then_body,
                    elsifs: Vec::new(),
                    elsen,
                })),
            }
        }
    }

    fn assemble_elif(
        &mut self,
        span: Span,
        mut after_elif: Vec<Toast>,
        indented: Vec<Toast>,
    ) -> Toast {
        // elif is like if but will be collected into an if/elif/else chain later
        if !indented.is_empty() {
            // Indental form: after_elif = condition tokens, indented = then-body
            self.pratt_parse_series(&mut after_elif, true); // parse one: just the condition
            let condition = if after_elif.is_empty() {
                self.errors.push(Error::new(span, "elif without condition"));
                Toast {
                    span,
                    v: ToastV::Error,
                }
            } else {
                if after_elif.len() > 1 {
                    self.errors.push(Error::new(
                        span,
                        "unexpected tokens after elif condition in indental form",
                    ));
                }
                after_elif.remove(0)
            };
            Toast {
                span,
                v: ToastV::Conditional(Box::new(Conditional {
                    condition,
                    then: indented,
                    elsifs: Vec::new(),
                    elsen: Vec::new(),
                })),
            }
        } else {
            // Paren form or inline: elif(condition then...)
            self.pratt_parse_series(&mut after_elif, false);
            if after_elif.is_empty() {
                self.errors.push(Error::new(span, "elif without condition"));
                return Toast {
                    span,
                    v: ToastV::Error,
                };
            }
            let condition = after_elif.remove(0);
            Toast {
                span,
                v: ToastV::Conditional(Box::new(Conditional {
                    condition,
                    then: after_elif,
                    elsifs: Vec::new(),
                    elsen: Vec::new(),
                })),
            }
        }
    }

    fn assemble_else(&mut self, span: Span, indented: Vec<Toast>) -> Toast {
        // else has no condition, just a body
        // We represent it as a Conditional with empty condition/then and populated elsen
        Toast {
            span,
            v: ToastV::Conditional(Box::new(Conditional {
                condition: Toast {
                    span,
                    v: ToastV::Dummy,
                },
                then: Vec::new(),
                elsifs: Vec::new(),
                elsen: indented,
            })),
        }
    }

    fn assemble_totion(
        &mut self,
        span: Span,
        mut after_to: Vec<Toast>,
        indented: Vec<Toast>,
    ) -> Toast {
        // Check for optional `:` before return type
        let (rt, mut inline_body) =
            if !after_to.is_empty() && matches!(&after_to[0].v, ToastV::Operator(s) if s == ":") {
                after_to.remove(0); // skip `:`
                self.pratt_parse_series(&mut after_to, true); // parse one: just the return type
                if after_to.is_empty() {
                    (None, Vec::new())
                } else {
                    let rt = after_to.remove(0);
                    (Some(Box::new(rt)), after_to)
                }
            } else {
                // No `:` — everything is body
                (None, after_to)
            };

        self.pratt_parse_series(&mut inline_body, false);
        inline_body.extend(indented);

        Toast {
            span,
            v: ToastV::Totion {
                return_type: rt,
                body: inline_body,
            },
        }
    }

    fn assemble_indental(
        &mut self,
        root_line: &mut Vec<Toast>,
        indented: Vec<Toast>,
    ) -> Option<Toast> {
        if root_line.is_empty() {
            root_line.extend(indented);
            return None;
        }

        // Scan for keywords, building a context-aware sequence stack.
        // `to` is only pushed as a Totion if it's NOT inside an open `fn` scope.
        let mut sequence_stack: Vec<SequenceStackEntry> = Vec::new();
        let mut has_open_fn = false;
        let mut _has_open_if = false;
        for (i, toast) in root_line.iter().enumerate() {
            if let Some(st) = SequenceType::from_toast(toast) {
                match st {
                    SequenceType::Totion if has_open_fn => {
                        // `to` within fn scope: part of fn's syntax, not an independent host
                    }
                    SequenceType::Function => {
                        has_open_fn = true;
                        sequence_stack.push(SequenceStackEntry { label: st, at: i });
                    }
                    SequenceType::Conditional => {
                        _has_open_if = true;
                        sequence_stack.push(SequenceStackEntry { label: st, at: i });
                    }
                    _ => {
                        sequence_stack.push(SequenceStackEntry { label: st, at: i });
                    }
                }
            }
        }

        let host_entry = sequence_stack.iter().find(|e| e.label.can_host_indental());

        if let Some(host) = host_entry {
            let host_at = host.at;
            let host_label = host.label;

            // Warn if multiple host-capable keywords are left open
            let host_count = sequence_stack
                .iter()
                .filter(|e| e.label.can_host_indental())
                .count();
            if host_count > 1 {
                self.errors.push(Error::new(
                    root_line[host_at].span,
                    "multiple keywords could claim indented block — using outermost",
                ));
            }

            // Drain keyword and everything after it
            let mut tail: Vec<Toast> = root_line.drain(host_at..).collect();
            let keyword_span = tail[0].span;
            tail.remove(0); // consume keyword atom

            let span = Span::from_range(
                keyword_span.start,
                indented.last().map_or(
                    tail.last().map_or(keyword_span.end(), |t| t.span().end()),
                    |t| t.span().end(),
                ),
            );

            let assembled = match host_label {
                SequenceType::Function => self.assemble_function(span, tail, indented),
                SequenceType::Conditional => self.assemble_conditional(span, tail, indented),
                SequenceType::Elif => self.assemble_elif(span, tail, indented),
                SequenceType::Else => self.assemble_else(span, indented),
                SequenceType::Block => {
                    let mut statements = tail;
                    statements.extend(indented);
                    Toast {
                        span,
                        v: ToastV::Block { statements },
                    }
                }
                SequenceType::Totion => self.assemble_totion(span, tail, indented),
                SequenceType::LeadOperator | SequenceType::TailOperator => {
                    // These aren't actually used as indental hosts
                    unreachable!()
                }
            };

            Some(assembled)
        } else {
            // No keyword host: fallback to invocation
            self.assemble_indental_fallback(root_line, indented);
            None
        }
    }

    fn assemble_indental_fallback(&mut self, toasts: &mut Vec<Toast>, indented: Vec<Toast>) {
        if toasts.is_empty() {
            toasts.extend(indented);
            return;
        }

        let last_idx = toasts.len() - 1;

        if matches!(&toasts[last_idx].v, ToastV::Operator(_)) {
            // Root line ends with operator: operator becomes the invocation head,
            // preceding operand (if any) + indented = args
            let op = toasts.pop().unwrap();
            let mut params = Vec::new();
            if !toasts.is_empty() && !matches!(&toasts.last().unwrap().v, ToastV::Operator(_)) {
                params.push(toasts.pop().unwrap());
            }
            params.extend(indented);
            let span = Span::from_range(
                op.span.start,
                params.last().map_or(op.span.end(), |t| t.span().end()),
            );
            toasts.push(Toast {
                span,
                v: ToastV::Invocation {
                    caller: Box::new(op),
                    kind: ParenType::Round,
                    parameters: params,
                },
            });
        } else {
            // Root line ends with non-operator: find the tail after the last operator.
            // The first item in the tail becomes the invocation head, the rest + indented = args.
            let last_op_pos = toasts
                .iter()
                .rposition(|t| matches!(&t.v, ToastV::Operator(_)));
            let tail_start = last_op_pos.map_or(0, |p| p + 1);

            let mut tail: Vec<Toast> = toasts.drain(tail_start..).collect();
            let head = tail.remove(0);
            let mut params = tail;
            params.extend(indented);

            let span = Span::from_range(
                head.span.start,
                params.last().map_or(head.span.end(), |t| t.span().end()),
            );
            toasts.push(Toast {
                span,
                v: ToastV::Invocation {
                    caller: Box::new(head),
                    kind: ParenType::Round,
                    parameters: params,
                },
            });
        }
    }

    /// Collect consecutive if/elif/else conditionals into a single Conditional node
    fn collect_conditional_chains(&mut self, toasts: &mut Vec<Toast>) {
        let mut i = 0;
        while i < toasts.len() {
            // Check if this is a Conditional (if statement)
            if let ToastV::Conditional(cond) = &toasts[i].v {
                // Check if it already has elsifs or elsen - if so, skip it
                if !cond.elsifs.is_empty() || !cond.elsen.is_empty() {
                    i += 1;
                    continue;
                }

                // Look ahead for elif/else statements
                let mut elsifs: Vec<(Toast, Vec<Toast>)> = Vec::new();
                let mut elsen: Vec<Toast> = Vec::new();
                let mut j = i + 1;

                // Collect consecutive elif/else statements
                while j < toasts.len() {
                    match &toasts[j].v {
                        ToastV::Conditional(elif_cond) => {
                            // Check if this looks like an elif or else
                            // elif would have a condition and then branch
                            // else would have no condition (just elsen branch)
                            if !elif_cond.elsen.is_empty() && elif_cond.then.is_empty() {
                                // This is an else - take its elsen branch
                                elsen = elif_cond.elsen.clone();
                                j += 1;
                                break; // else must be last
                            } else if !elif_cond.then.is_empty() {
                                // This looks like an elif
                                elsifs.push((elif_cond.condition.clone(), elif_cond.then.clone()));
                                j += 1;
                            } else {
                                break; // Not a continuation
                            }
                        }
                        ToastV::Comment { .. } => {
                            // Skip comments between if/elif/else
                            j += 1;
                        }
                        ToastV::Dummy => {
                            j += 1;
                        }
                        _ => break, // Not a conditional continuation
                    }
                }

                // If we found any elif/else, merge them into the if
                if !elsifs.is_empty() || !elsen.is_empty() {
                    // Remove the original if and all the elif/else nodes we consumed
                    let original_if = toasts.remove(i);
                    for _ in 0..(j - i - 1) {
                        toasts.remove(i);
                    }

                    // Create new merged conditional
                    if let ToastV::Conditional(if_cond) = original_if.v {
                        let merged = Toast {
                            span: original_if.span,
                            v: ToastV::Conditional(Box::new(Conditional {
                                condition: if_cond.condition,
                                then: if_cond.then,
                                elsifs,
                                elsen,
                            })),
                        };
                        toasts.insert(i, merged);
                    }
                }
            }
            i += 1;
        }
    }
}

// converts every toast into a ToastAst, or reports errors

#[derive(Clone, Copy)]
enum SequenceType {
    Conditional,
    Function,
    // argless function (or where the args are free variables) started with 'to'
    Totion,
    // when an operator is at the beginning of a root line
    LeadOperator,
    TailOperator,
    Block,
    Elif,
    Else,
}
impl SequenceType {
    fn from_label(label: &str) -> Option<Self> {
        match label {
            "fn" => Some(Self::Function),
            "to" => Some(Self::Totion),
            "if" => Some(Self::Conditional),
            "elif" => Some(Self::Elif),
            "else" => Some(Self::Else),
            "do" => Some(Self::Block),
            _ => None,
        }
    }
    fn from_toast(toast: &Toast) -> Option<Self> {
        match &toast.v {
            ToastV::Atom { value } => Self::from_label(value),
            _ => None,
        }
    }
    fn can_host_indental(&self) -> bool {
        true
    }
}
struct SequenceStackEntry {
    label: SequenceType,
    at: usize,
}

/// Process a sequence of toasts, resolving keywords, operators, and indental structure.
/// `indented`: if Some, this is the root line of an Indental and the Vec is the indented content.
/// returns the outermost sequence type that was left open
/// remember to check if errors became longer

// ============================================================================
// Pratt Parsing
// ============================================================================

fn dummy_toast() -> Toast {
    Toast {
        span: Span::new(0, 0),
        v: ToastV::Dummy,
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Parse source code into AST nodes.
/// This is the main entry point for parsing.
pub fn parse(source: &str, operator_table: &OperatorTable) -> Result<Vec<Ast>, Vec<Error>> {
    // First, tokenize into Toasts
    let toasts = sequence(source, operator_table)?;

    // Then structure into AST
    let structurer = Structurer::new(toasts, operator_table.clone());
    let toasts = structurer.structure()?;

    // Verify all toasts are valid ASTs
    let mut errors = Vec::new();
    for toast in &toasts {
        toast.verify_ast_writer(&mut errors);
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    // Convert to pure Ast type
    toasts
        .into_iter()
        .map(|t| t.to_ast())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| vec![e])
}

/// Tokenize and structure source code into Toast nodes (but don't convert to pure AST).
/// This is useful for testing the parsing behavior while keeping Toast structure.
pub fn sequence_structured(
    source: &str,
    operator_table: &OperatorTable,
) -> Result<Vec<Toast>, Vec<Error>> {
    // First, tokenize into Toasts
    let toasts = sequence(source, operator_table)?;

    // Then structure into Toast ASTs
    let structurer = Structurer::new(toasts, operator_table.clone());
    structurer.structure()
}
