

// overview: This is a very flexible parser for languages that may or may not use indentation to structure, which have a fixed number of infix operators, with precedence. This was entirely sufficient to implement the quite sophisticated syntax of the ~~Bjork~~Kaba language.
// the tokenizer converts a string into Scrips, which are sometimes like tokens, other times richer and more structured, as they can retain indent structure. The Scrips are then transformed through term rewriting into Asts.
// keywords: fn, to, if, else, elif


use crate::{Ref, Arena};
/*
The example input:

# wait, this function syntax doesn't work how I wish it did. How should I do a function syntax... i see no good options. I am stuck. What value is there in this if it cannot be beautiful?
  # this tantrum evolved into "how can we keep the parser minimalistic and flexible" and the answer turned out to be "it should mostly be term rewriting over a tokenizer, though the tokenization may not just be tokens, it may retain indent structure and stuff"

f = fn(a:int b:int to:int a + b)
print(f(1 2))
# this is a comment
#(
  this is a multiline comment
)
combined = struct(a:int b:int)
# various musing on function syntax
fc = fn(c:combined to:int c.a + c.b)

fc = fn c:combined to int
    c.a + c.b

ac =
    if c
        c.a + c.b
    else 0

*/

// ============================================================================
// AST Types
// ============================================================================

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
    Token(String),
    Quoted(QuoteType, String),
    /// Invocation: paren type, optional caller (the token before the paren), and arguments
    Invocation(ParenType, Option<Box<Toast>>, Vec<Toast>),
    Indental {
        root_line: Vec<Toast>,
        indented: Vec<Toast>,
    },
}

/// Tracks the content accumulated at a given indentation level
struct IndentLevel {
    /// Length of the indent prefix within the parent InvocationLevel's known_indent
    indent_len: usize,
    /// The scrips accumulated at this indent level (the root line, before any sub-indent)
    root_line: Vec<Toast>,
    /// The scrips accumulated in indented sub-content
    indented: Vec<Toast>,
    /// Span start for this indent level
    span_start: usize,
}

/// Tracks open invocations (paren groups) and their associated indent state
struct InvocationLevel {
    /// None for root level, Some for actual parens
    paren_type: Option<ParenType>,
    /// The token that preceded the opening paren (the "caller"), if any
    caller: Option<Toast>,
    span_start: usize,
    /// The known indent string - extended as indent_stack grows, shortened as it pops
    known_indent: String,
    /// Indent stack for this invocation level
    indent_stack: Vec<IndentLevel>,
    /// Content accumulated inside this invocation
    content: Vec<Toast>,
}

impl InvocationLevel {
    /// Get the current output destination within this invocation level.
    /// Uses `indented` if it's non-empty (meaning we've seen sub-content), otherwise `root_line`.
    fn get_output(&mut self) -> &mut Vec<Toast> {
        if let Some(indent) = self.indent_stack.last_mut() {
            if !indent.indented.is_empty() {
                &mut indent.indented
            } else {
                &mut indent.root_line
            }
        } else {
            &mut self.content
        }
    }

    /// Flush all indent levels, collapsing them into content
    fn flush_indents(&mut self, end_pos: usize) {
        while let Some(finished) = self.indent_stack.pop() {
            finalize_indent_level_into(
                &mut self.indent_stack,
                &mut self.content,
                finished,
                end_pos,
            );
        }
    }
}

/// Finalize a popped indent level, either merging or creating an Indental
fn finalize_indent_level(paren: &mut InvocationLevel, finished: IndentLevel, end_pos: usize) {
    finalize_indent_level_into(
        &mut paren.indent_stack,
        &mut paren.content,
        finished,
        end_pos,
    );
}

/// Helper to finalize an indent level into the given parent stack/content
fn finalize_indent_level_into(
    indent_stack: &mut Vec<IndentLevel>,
    content: &mut Vec<Toast>,
    finished: IndentLevel,
    end_pos: usize,
) {
    if finished.indented.is_empty() {
        // No sub-content at this level - just merge root_line into parent's indented
        // (since this level was sub-content of the parent)
        if let Some(parent) = indent_stack.last_mut() {
            parent.indented.extend(finished.root_line);
        } else {
            content.extend(finished.root_line);
        }
    } else {
        // Has sub-content, create an Indental
        let tokk = Toast::Tokk(Tokk {
            span: Span::from_range(finished.span_start, end_pos),
            content: TokkV::Indental {
                root_line: finished.root_line,
                indented: finished.indented,
            },
        });
        if let Some(parent) = indent_stack.last_mut() {
            parent.indented.push(tokk);
        } else {
            content.push(tokk);
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

mod ast {
    use crate::parser::Span;

#[derive(Debug)]
/// AL is [Ast] node Link type. Before the parser it'll be [Toast]s, after the parser it'll be Box<Ast>s. During type checking you'll probably want it to be graph node ids.
pub struct Ast {
    pub span: Span,
    pub v: AstV,
}

#[derive(Debug)]
enum AstV {
    Invocation(Invocation),
    Comment(Comment),
    Conditional(Conditional),
    Function {
        args: Vec<Box<Ast>>,
        return_type: Option<Box<Ast>>,
        body: Box<Ast>,
    },
    Block {
        statements: Vec<Box<Ast>>,
    },
}

#[derive(Debug)]
struct Conditional {
    condition: Box<Ast>,
    then: Box<Ast>,
    elsen: Option<Box<Ast>>,
    elsifs: Vec<(Box<Ast>, Box<Ast>)>,
}

#[derive(Debug)]
pub struct Invocation {
    pub span: Span,
    pub head: String,
    pub parentheticals: Vec<Vec<Ast>>,
}

#[derive(Debug)]
pub struct Operator {
    pub span: Span,
    pub name: String,
    pub arguments: Vec<Ast>,
}

#[derive(Debug)]
pub struct Comment {
    pub span: Span,
    pub content: String,
}

#[derive(Debug)]
pub struct Atom {
    pub span: Span,
    pub value: String,
}
}

pub use ast::Ast;

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
    '=', ':', '+', '-', '*', '/', '%', '<', '>', '!', '.', '&', '|', '^', ';', '~', '@', '$', '?',
];

/// Entry in the operator lookup table, indexed by first character of operator.
#[derive(Debug, Clone)]
pub enum OperatorEntry {
    /// This character can never be an operator (alphanumeric, whitespace, etc.)
    Alphanumeric,
    /// This character could be an operator but isn't used in the current operator list
    OperatorNotUsed,
    /// Exactly one operator starts with this character.
    /// Stores (full operator string, precedence) for verification.
    Single(String, u16),
    /// Multiple operators start with this character, need disambiguation.
    /// Stores vec of (full operator string, precedence).
    Contended(Vec<(String, u16)>),
}

/// Fast operator precedence lookup table.
/// Uses a 128-element array indexed by first character (ASCII).
#[derive(Debug, Clone)]
pub struct OperatorTable {
    entries: Vec<OperatorEntry>,
}

impl OperatorTable {
    /// Build an operator table from a list of operators.
    /// Operators are given in precedence order: index 0 = lowest precedence.
    pub fn new(operators: Vec<String>) -> Self {
        use std::collections::HashMap;

        // Initialize all entries as Alphanumeric
        let mut entries: Vec<OperatorEntry> = vec![OperatorEntry::Alphanumeric; 128];

        // Mark all potential operator characters as OperatorNotUsed
        for &c in OPERATOR_CHARS {
            let idx = c as usize;
            if idx < 128 {
                entries[idx] = OperatorEntry::OperatorNotUsed;
            }
        }

        // Group operators by their first character
        let mut by_first_char: HashMap<char, Vec<(String, u16)>> = HashMap::new();

        for (prec, op) in operators.into_iter().enumerate() {
            if let Some(first_char) = op.chars().next() {
                by_first_char
                    .entry(first_char)
                    .or_default()
                    .push((op, prec as u16));
            }
        }

        // Convert grouped operators into entries
        for (c, ops) in by_first_char {
            let idx = c as usize;
            if idx < 128 {
                entries[idx] = if ops.len() == 1 {
                    let (op, prec) = ops.into_iter().next().unwrap();
                    OperatorEntry::Single(op, prec)
                } else {
                    OperatorEntry::Contended(ops)
                };
            }
        }

        Self { entries }
    }

    /// Look up an operator's precedence.
    /// Returns None if the operator is not in the table.
    pub fn lookup(&self, op: &str) -> Option<u16> {
        let first_char = op.chars().next()?;
        let idx = first_char as usize;

        if idx >= 128 {
            return None;
        }

        match &self.entries[idx] {
            OperatorEntry::Alphanumeric | OperatorEntry::OperatorNotUsed => None,
            OperatorEntry::Single(stored_op, prec) => {
                if stored_op == op {
                    Some(*prec)
                } else {
                    None
                }
            }
            OperatorEntry::Contended(ops) => {
                ops.iter().find(|(s, _)| s == op).map(|(_, prec)| *prec)
            }
        }
    }

    /// Check if a character is an operator character (could be part of an operator).
    /// This is true for OperatorNotUsed, Single, and Contended entries.
    #[inline]
    pub fn is_operator_char(&self, c: char) -> bool {
        let idx = c as usize;
        if idx >= 128 {
            return false;
        }
        !matches!(self.entries[idx], OperatorEntry::Alphanumeric)
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



fn sequence(source: &str, operators: &OperatorTable) -> Result<Vec<Toast>, Error> {
    use std::iter::Peekable;
    use std::str::Chars;

    let mut pos: usize = 0;
    let mut chars: Peekable<Chars> = source.chars().peekable();

    // Stack of open invocations - bottom entry (None type) is never popped
    let mut invocation_stack: Vec<InvocationLevel> = vec![InvocationLevel {
        caller: None,
        paren_type: None,
        span_start: 0,
        known_indent: String::new(),
        indent_stack: Vec::new(),
        content: Vec::new(),
    }];

    // Track if we're at the start of a line (for indent processing)
    let mut at_line_start = true;

    while chars.peek().is_some() {
        // Handle line starts - measure indentation
        if at_line_start {
            let indent_start = pos;
            let current = invocation_stack.last_mut().unwrap();
            let base_len = if let Some(l) = current.indent_stack.last() { l.indent_len } else { 0 };
            
            // Read whitespace, comparing against known_indent and extending if deeper
            let mut new_len = 0;
            let mut mismatch_at: Option<usize> = None;
            
            loop {
                match chars.peek() {
                    Some(&' ') | Some(&'\t') => {
                        let c = *chars.peek().unwrap();
                        chars.next();
                        pos += 1;
                        
                        if new_len < base_len {
                            // Compare against existing pattern
                            if current.known_indent.as_bytes()[new_len] != c as u8 {
                                if mismatch_at.is_none() {
                                    mismatch_at = Some(new_len);
                                }
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
                        mismatch_at = None;
                    }
                    Some(&'\n') => {
                        // Empty line - consume and restart
                        chars.next();
                        pos += 1;
                        current.known_indent.truncate(base_len);
                        new_len = 0;
                        mismatch_at = None;
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
            let is_bracket = matches!(next_char, Some(')') | Some(']') | Some('}') | Some('(') | Some('[') | Some('{'));
            
            if is_bracket {
                // Closing bracket - truncate any extension
                current.known_indent.truncate(base_len);
            }else{
                // Check for inconsistent indentation (mismatch within the prefix we should match)
                if let Some(mm) = mismatch_at {
                    // Mismatch at position mm - check if we're dedenting to a level <= mm
                    // Find if any level has indent_len <= mm
                    let valid_dedent = current.indent_stack.iter()
                        .any(|l| l.indent_len <= mm);
                    
                    if !valid_dedent || new_len > mm {
                        return Err(Error::new(
                            Span::new(indent_start, new_len.max(1)),
                            "Inconsistent indentation: whitespace pattern doesn't match previous levels",
                        ));
                    }
                }

                // Pop levels where new_len is less than their indent_len
                while let Some(top) = current.indent_stack.last() {
                    if new_len < top.indent_len {
                        // Dedenting past this level
                        let finished = current.indent_stack.pop().unwrap();
                        let parent_len = current.indent_stack.last().map(|l| l.indent_len).unwrap_or(0);
                        current.known_indent.truncate(parent_len);
                        finalize_indent_level(current, finished, pos);
                    } else if new_len == top.indent_len {
                        // Same level
                        if !top.indented.is_empty() {
                            // Had indented content = start new block
                            let finished = current.indent_stack.pop().unwrap();
                            let parent_len = current.indent_stack.last().map(|l| l.indent_len).unwrap_or(0);
                            current.known_indent.truncate(parent_len);
                            finalize_indent_level(current, finished, pos);
                        } else {
                            // Continue in same block, truncate any extension
                            current.known_indent.truncate(new_len);
                            break;
                        }
                    } else {
                        // new_len > top.indent_len - deeper, will push below
                        break;
                    }
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
                        root_line: Vec::new(),
                        indented: Vec::new(),
                        span_start: pos,
                    });
                }
            }

            // Continue to parse the actual content
            match chars.peek() {
                None => break,
                Some('\n') | Some('\r') => continue,
                _ => {}
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
                let mut caller = {
                    let current = invocation_stack.last_mut().unwrap();
                    current.get_output().pop()
                };

                // If no caller found and we're at a fresh indent level (nothing on this line yet),
                // try stealing from the parent indent level. This handles:
                //   foo
                //     (bar)  -> becomes foo(bar) instead of Indental{foo, [(bar)]}
                if caller.is_none() {
                    let current = invocation_stack.last_mut().unwrap();
                    if let Some(indent) = current.indent_stack.last() {
                        // Check if current indent level is empty (we just increased indent)
                        if indent.root_line.is_empty() && indent.indented.is_empty() {
                            // Pop the empty indent level and steal from parent
                            if current.indent_stack.len() >= 2 {
                                current.indent_stack.pop(); // Remove empty level
                                // Truncate known_indent to parent's length
                                let new_len = current.indent_stack.last().map(|l| l.indent_len).unwrap_or(0);
                                current.known_indent.truncate(new_len);
                                // Now try to steal from parent level's root_line
                                if let Some(parent) = current.indent_stack.last_mut() {
                                    caller = parent.root_line.pop();
                                    // Parent's state is fine - if it becomes empty, !indented.is_empty()
                                    // will be false anyway
                                }
                            }
                        }
                    }
                }

                // Only allow None caller at file root level (invocation_stack.len() == 1)
                // and only if we're not inside any indent structure
                let at_file_root = invocation_stack.len() == 1;
                if caller.is_none() && !at_file_root {
                    return Err(Error::new(
                        Span::new(pos, 1),
                        format!("Opening '{}' requires a caller - nothing precedes it", c),
                    ));
                }

                // Adjust span_start to include the caller if present
                let span_start = match &caller {
                    Some(t) => t.span().start,
                    None => pos,
                };

                invocation_stack.push(InvocationLevel {
                    paren_type: Some(paren_type),
                    caller,
                    span_start,
                    known_indent: String::new(),
                    indent_stack: Vec::new(),
                    content: Vec::new(),
                });

                chars.next();
                pos += 1;
            }

            // Closing brackets - complete an invocation
            ')' | ']' | '}' => {
                let expected_type = ParenType::from_close(c);

                // Check we're not trying to close the root
                if invocation_stack.len() <= 1 {
                    return Err(Error::new(
                        Span::new(pos, 1),
                        format!("Unmatched closing bracket: '{}'", c),
                    ));
                }

                let mut entry = invocation_stack.pop().unwrap();

                // Check for matching paren type
                let entry_type = entry.paren_type.unwrap(); // Safe: not root
                if entry_type != expected_type {
                    let expected_char = entry_type.close_char();
                    return Err(Error::new(
                        Span::new(pos, 1),
                        format!(
                            "Mismatched bracket: expected '{}', found '{}'",
                            expected_char, c
                        ),
                    ));
                }

                // Flush any remaining indents inside this invocation
                entry.flush_indents(pos);

                // Box the caller if present
                let caller_boxed = entry.caller.map(Box::new);

                // Create the Invocation tokk
                let tokk = Tokk {
                    span: Span::from_range(entry.span_start, pos + 1),
                    content: TokkV::Invocation(entry_type, caller_boxed, entry.content),
                };

                // Add to parent's output
                let parent = invocation_stack.last_mut().unwrap();
                parent.get_output().push(Toast::Tokk(tokk));

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
                        return Err(Error::new(
                            Span::from_range(start, pos),
                            format!("Unclosed {} string literal", quote_char),
                        ));
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
                                return Err(Error::new(
                                    Span::from_range(start, pos),
                                    format!("Unclosed {} string literal", quote_char),
                                ));
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
                                    return Err(Error::new(
                                        Span::new(pos, 1),
                                        format!("Invalid escape sequence: \\{}", escaped),
                                    ));
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
                let current = invocation_stack.last_mut().unwrap();
                current.get_output().push(Toast::Tokk(tokk));
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
                    content: TokkV::Token(op),
                };
                let current = invocation_stack.last_mut().unwrap();
                current.get_output().push(Toast::Tokk(tokk));
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
                        let Some(ch) = chars.next() else { break };
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

                    if depth > 0 {
                        return Err(Error::new(
                            Span::from_range(start, pos),
                            "Unclosed multi-line comment",
                        ));
                    }

                    let tokk = Tokk {
                        span: Span::from_range(start, pos),
                        content: TokkV::Token(content),
                    };
                    let current = invocation_stack.last_mut().unwrap();
                    current.get_output().push(Toast::Tokk(tokk));
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

                    let tokk = Tokk {
                        span: Span::from_range(start, pos),
                        content: TokkV::Token(content),
                    };
                    let current = invocation_stack.last_mut().unwrap();
                    current.get_output().push(Toast::Tokk(tokk));
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
                    let tokk = Tokk {
                        span: Span::from_range(start, pos),
                        content: TokkV::Token(value),
                    };
                    let current = invocation_stack.last_mut().unwrap();
                    current.get_output().push(Toast::Tokk(tokk));
                }
            }
        }
    }

    // Check for unclosed parens (any beyond the root)
    if invocation_stack.len() > 1 {
        let entry = &invocation_stack[invocation_stack.len() - 1];
        let paren_char = entry.paren_type.unwrap().close_char();
        return Err(Error::new(
            Span::new(entry.span_start, 1),
            format!("Unclosed bracket: '{}'", paren_char),
        ));
    }

    // Flush remaining indent stack in root and return its content
    let mut root = invocation_stack.pop().unwrap();
    root.flush_indents(pos);
    Ok(root.content)
}

/// Token Or Ast
#[derive(Debug)]
enum Toast {
    Tokk(Tokk),
    Ast(ToastAst),
}
impl Toast {
    fn as_tokk(&self) -> &Tokk {
        match self {
            Toast::Tokk(tokk) => tokk,
            Toast::Ast(ast) => panic!("Toast is not a Tokk"),
        }
    }  
    fn as_ast(&self) -> &ToastAst {
        match self {
            Toast::Tokk(tokk) => panic!("Toast is not an Ast"),
            Toast::Ast(ast) => ast,
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
/// ToastAst is like ast::Ast but links Toasts instead of Asts for parser rewriting.
pub struct ToastAst {
    pub span: Span,
    pub v: ToastAstV,
}

#[derive(Debug)]
pub enum ToastAstV {
    Invocation {
        span: Span,
        head: String,
        parentheticals: Vec<Vec<Box<Toast>>>,
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
    Operator {
        span: Span,
        name: String,
        arguments: Vec<Box<Toast>>,
    },
    Atom {
        span: Span,
        value: String,
    },
}


/**
Takes a sequence of [Toast]s that are initially all [Tokk]s and applies some rewrite rules to transform them into [Ast] Toasts, which are then stripped down into a tree of pure Asts.
for each `for`, either the result must already be in the to [final state] form, or one of the following rules must match around its key term (which then take it to that final state), and that rule transforms it into an Ast. If no rule matches under a for expression, this is a syntax error. If there are no syntax errors, all remaining Tokks are converted to Tokens and the Ast is complete.
`%reverse` means it's greedy but from the other direction, processing terms from the right first
The macro matching rule syntax here is pretty much taken from rust.

# first, if there are operator-llinked things within an indental head with a non-operator term at the end, the indental belongs to that non-operator stuff at the end
%indental($o:operators $(x)?)($(y)*) → $o($x $y)
%indental(a@$($_ $_:operators)+ $y*)($z*) → $a %indental($y)($z))

# convert all infix operator expressions to invocation asts
for $o in operators:
    $x $o $y → $o($x $y)
    %indental($x* $o)($(y)*) → $o($x $y)

# we define if elif then as three match functions that can match and convert the parts, whatever form they take
def %if → if(condition($c) then($x))
    if $c $x
    %indental(if $c)($x*)
    if($c $x*)
def %else → else($x)
    else $x
    %indental(else)($x*)
def %elif → elif(condition($c) then($x))
    elif $c $x
    %indental(elif $c)($x*)
    elif($c $x*)

for "if"
    %if $(%elif)* $(%else)?

for "do" to do($doings*)
    %indental(do $predoings*)($doings*) → do($predoings* $doings*)
    
for "fn"
    to fn(parameters($parameters*) body($doings*))
        fn($parameters* to $doings*)
        %indental(fn $parameters* $(to)?)($doings*)
    to fn(parameters($parameters*) returnType($return)) body($doings*)
        fn($parameters* to:$return $doings*)
        %indental(fn $parameters* to $(:)? $return)($doings*)
    
It then converts all remaining indentals into function invocations, and then all remaining token parens into invocations, and all remaining tokens into the corresponding ast.
Hmm maybe the paren to invocation step should happen in the sequence stage
*/
/*
// TODO: Incomplete work-in-progress code - commented out to allow tests to compile
fn structure(mut tokks: Vec<Toast>, operators:&[String], operator_table: &OperatorTable, rules: &[KeywordRule]) -> Result<Arena, Error> {
    
    
    let mut arena = Arena::new();
    
    for tokk in tokks {
        let mut toast = tokk;
        for rule in rules {
            if rule.keywords.contains(&toast.as_tokk().content.to_string()) {
                toast = rule.rule(&mut toast)?;
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
    rule: fn(t:& mut Toast) -> Result<Toast, Error>,
}

/// operators are ordered from highest to lowest precedence
fn parse(content: &str, operators:&[String], rules: &[KeywordRule]) -> Result<Arena, Error> {
    let operator_table = OperatorTable::new(operators.to_vec());
    let tokks = sequence(content, &operator_table)?;
    structure(tokks, operators, &operator_table, &rules)
}

fn parse_language(content: &str)-> Result<Arena, Error> {
    parse(content, &[
        ".".into(),
        ":".into(),
        "@".into(),
        "/".into(),
        "*".into(),
        "-".into(),
        "+".into(),
        "==".into(),
        "!=".into(),
        "<".into(),
        ">".into(),
        "<=".into(),
        ">=".into(),
        "&&".into(),
        "||".into(),
        "=".into(),
    ],
    &[])
}
*/

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Tokenizer Tests
    // ========================================================================

    /// Helper to extract token string from Scrip
    fn as_token(tokk: &Toast) -> Option<&str> {
        match &tokk {
            Toast::Tokk(Tokk { content: TokkV::Token(s), .. }) => Some(s),
            _ => None,
        }
    }

    /// Helper to extract token string from Tokk directly
    fn as_token_from_tokk(tokk: &Tokk) -> Option<&str> {
        match &tokk.content {
            TokkV::Token(s) => Some(s),
            _ => None,
        }
    }

    /// Helper to extract quoted string from tokk
    fn as_quoted(tokk: &Toast) -> Option<(QuoteType, &str)> {
        match &tokk {
            Toast::Tokk(Tokk { content: TokkV::Quoted(qt, s), .. }) => Some((*qt, s)),
            _ => None,
        }
    }

    /// Helper to extract invocation content from tokk
    fn as_invocation(tokk: &Toast) -> (ParenType, Option<&Toast>, &Vec<Toast>) {
        match &tokk {
            Toast::Tokk(Tokk { content: TokkV::Invocation(pt, caller, content), .. }) => {
                (*pt, caller.as_ref().map(|b| b.as_ref()), content)
            }
            _ => panic!("Toast is not an Invocation"),
        }
    }

    /// Helper to extract indental from tokk
    fn as_indental(tokk: &Toast) -> Option<(&Vec<Toast>, &Vec<Toast>)> {
        match &tokk {
            Toast::Tokk(Tokk { content: TokkV::Indental { root_line, indented }, .. }) => Some((root_line, indented)),
            _ => None,
        }
    }

    fn make_operator_table() -> OperatorTable {
        OperatorTable::new(vec![
            "=".to_string(),
            "+".to_string(),
            "-".to_string(),
            "*".to_string(),
            "/".to_string(),
            ":".to_string(),
            ".".to_string(),
        ])
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
        assert!(caller.is_some());
        assert_eq!(as_token(caller.unwrap()), Some("foo"));
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
        assert!(caller.is_some());
        assert_eq!(as_token(caller.unwrap()), Some("arr"));
        assert_eq!(content.len(), 1);
        assert_eq!(as_token(&content[0]), Some("0"));
    }

    #[test]
    fn test_tokenize_curly_brackets() {
        let ops = make_operator_table();
        let tokks = sequence("{a b}", &ops).expect("should tokenize");
        // {a b} has no caller
        assert_eq!(tokks.len(), 1);

        let (paren_type, caller, content) = as_invocation(&tokks[0]);
        assert!(matches!(paren_type, ParenType::Curly));
        assert!(caller.is_none());
        assert_eq!(content.len(), 2);
        assert_eq!(as_token(&content[0]), Some("a"));
        assert_eq!(as_token(&content[1]), Some("b"));
    }

    #[test]
    fn test_tokenize_mismatched_parens() {
        let ops = make_operator_table();
        let result = sequence("(a]", &ops);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Mismatched"));
    }

    #[test]
    fn test_tokenize_unclosed_paren() {
        let ops = make_operator_table();
        let result = sequence("(a b", &ops);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Unclosed"));
    }

    #[test]
    fn test_tokenize_unmatched_close() {
        let ops = make_operator_table();
        let result = sequence("a)", &ops);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Unmatched"));
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
        assert!(as_token(&scrips[1]).unwrap().starts_with("#"));
        assert_eq!(as_token(&scrips[2]), Some("b"));
    }

    #[test]
    fn test_tokenize_multiline_comment() {
        let ops = make_operator_table();
        let scrips = sequence("a #(multi\nline) b", &ops).expect("should tokenize");
        assert_eq!(scrips.len(), 3);
        assert_eq!(as_token(&scrips[0]), Some("a"));
        let comment = as_token(&scrips[1]).unwrap();
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
        assert!(outer_caller.is_some());
        assert_eq!(as_token(outer_caller.unwrap()), Some("f"));
        // Inside f(...) we have g(x) which is one invocation
        assert_eq!(outer_content.len(), 1);

        let (_, inner_caller, inner_content) = as_invocation(&outer_content[0]);
        assert!(inner_caller.is_some());
        assert_eq!(as_token(inner_caller.unwrap()), Some("g"));
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
        assert!(caller.is_some());
        assert_eq!(as_token(caller.unwrap()), Some("f"));
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
        assert!(caller.is_some());
        assert_eq!(as_token(caller.unwrap()), Some("f"));
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
        let (inner_root, inner_indented) = as_indental(&indented[0]).expect("should be nested indental");
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
        assert!(caller.is_some());
        assert_eq!(as_token(caller.unwrap()), Some("f"));
        assert_eq!(as_token(&content[0]), Some("x"));
        assert_eq!(as_token(&indented[1]), Some("b"));
    }

    #[test]
    fn test_tokenize_deeply_nested_indent() {
        let ops = make_operator_table();
        let scrips = sequence("a\n  b\n    c\n      d", &ops).expect("should tokenize");

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
        assert!(err.message.contains("Inconsistent indentation"));
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
        assert!(caller.is_some());
        assert_eq!(as_token(caller.unwrap()), Some("b"));
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
        assert!(err.message.contains("Inconsistent indentation"));
    }
    
    

    #[test]
    fn test_tokenize_prefix_indent_works() {
        let ops = make_operator_table();
        // First level is 2 spaces, second level is 4 spaces (extends the first)
        let scrips = sequence("a\n  b\n    c", &ops).expect("should tokenize");

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
    fn test_tokenize_paren_without_caller_error_inside_paren() {
        let ops = make_operator_table();
        // Inside a paren, another paren without a caller is an error
        let result = sequence("f(\n  (x)\n)", &ops);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("requires a caller"));
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
        assert!(caller.is_some());
        assert_eq!(as_token(caller.unwrap()), Some("foo"));
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
        assert!(caller.is_some());
        assert_eq!(as_token(caller.unwrap()), Some("foo"));
        assert_eq!(content.len(), 3);
        assert_eq!(as_token(&content[0]), Some("a"));
        assert_eq!(as_token(&content[1]), Some("b"));
        assert_eq!(as_token(&content[2]), Some("c"));
    }

    #[test]
    fn test_tokenize_curly_at_root_no_caller_ok() {
        let ops = make_operator_table();
        // At file root level, a paren/curly with no caller is allowed
        let scrips = sequence("{a b}", &ops).expect("should tokenize");

        assert_eq!(scrips.len(), 1);
        let (paren_type, caller, content) = as_invocation(&scrips[0]);
        assert!(matches!(paren_type, ParenType::Curly));
        assert!(caller.is_none()); // No caller at root is OK
        assert_eq!(content.len(), 2);
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
        assert!(err.message.contains("requires a caller"));
    }
}
