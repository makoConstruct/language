// overview: This is a very flexible parser for languages that may or may not use indentation to structure, which have a fixed number of infix operators, with precedence. This was entirely sufficient to implement the quite sophisticated syntax of the ~~Bjork~~Kaba language.
// the tokenizer converts a string into Scrips, which are sometimes like tokens, other times richer and more structured, as they can retain indent structure. The Scrips are then transformed through term rewriting into Asts.
// keywords: fn, to, if, else, elif

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
    Parens(ParenType, Vec<Toast>),
    Indental {
        root_line: Vec<Toast>,
        indented: Vec<Toast>,
    },
}

/// Tracks the content accumulated at a given indentation level
struct IndentLevel {
    /// The whitespace prefix string for this indent level
    whitespace: String,
    /// The scrips accumulated at this indent level (the root line, before any sub-indent)
    root_line: Vec<Toast>,
    /// The scrips accumulated in indented sub-content
    indented: Vec<Toast>,
    /// Whether we've moved past the root line into indented content
    in_indented: bool,
    /// Span start for this indent level
    span_start: usize,
}

/// tracks open parentheses and their associated indent state
struct ParenLevel {
    /// None for root level, Some for actual parens
    paren_type: Option<ParenType>,
    span_start: usize,
    /// Indent stack for this paren level
    indent_stack: Vec<IndentLevel>,
    /// Content accumulated inside this paren
    content: Vec<Toast>,
}

impl ParenLevel {
    /// Get the current output destination within this paren entry
    fn get_output(&mut self) -> &mut Vec<Toast> {
        if let Some(indent) = self.indent_stack.last_mut() {
            if indent.in_indented {
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
fn finalize_indent_level(paren: &mut ParenLevel, finished: IndentLevel, end_pos: usize) {
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
    if finished.indented.is_empty() && !finished.in_indented {
        // No indented content, merge root_line into parent
        if let Some(parent) = indent_stack.last_mut() {
            if parent.in_indented {
                parent.indented.extend(finished.root_line);
            } else {
                parent.root_line.extend(finished.root_line);
            }
        } else {
            content.extend(finished.root_line);
        }
    } else {
        // Create an Indental
        let tokk = Toast::Tokk(Tokk {
            span: Span::from_range(finished.span_start, end_pos),
            content: TokkV::Indental {
                root_line: finished.root_line,
                indented: finished.indented,
            },
        });
        if let Some(parent) = indent_stack.last_mut() {
            if parent.in_indented {
                parent.indented.push(tokk);
            } else {
                parent.root_line.push(tokk);
            }
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

#[derive(Debug)]
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
    pub parentheticals: Vec<Vec<Box<Ast>>>,
}

#[derive(Debug)]
pub struct Operator {
    pub span: Span,
    pub name: String,
    pub arguments: Vec<Box<Ast>>,
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

    // Stack of open parentheses - bottom entry (None type) is never popped
    let mut paren_stack: Vec<ParenLevel> = vec![ParenLevel {
        paren_type: None,
        span_start: 0,
        indent_stack: Vec::new(),
        content: Vec::new(),
    }];

    // Track if we're at the start of a line (for indent processing)
    let mut at_line_start = true;

    while chars.peek().is_some() {
        // Handle line starts - measure indentation
        if at_line_start {
            // Collect leading whitespace as a string
            let indent_start = pos;
            let mut indent_ws = String::new();
            loop {
                match chars.peek() {
                    Some(&' ') => {
                        chars.next();
                        pos += 1;
                        indent_ws.push(' ');
                    }
                    Some(&'\t') => {
                        chars.next();
                        pos += 1;
                        indent_ws.push('\t');
                    }
                    Some(&'\r') => {
                        // Empty line - consume and restart
                        chars.next();
                        pos += 1;
                        if chars.peek() == Some(&'\n') {
                            chars.next();
                            pos += 1;
                        }
                        indent_ws.clear();
                        // Continue measuring from start of next line
                    }
                    Some(&'\n') => {
                        // Empty line - consume and restart
                        chars.next();
                        pos += 1;
                        indent_ws.clear();
                        // Continue measuring from start of next line
                    }
                    _ => break,
                }
            }

            at_line_start = false;

            if chars.peek().is_none() {
                break;
            }

            // Check if the first non-whitespace char is a closing bracket.
            // If so, skip indent processing - the bracket will close the current paren,
            // and any remaining content belongs to the outer context.
            let next_char = chars.peek();
            let is_closing_bracket = matches!(next_char, Some(')') | Some(']') | Some('}'));
            
            if !is_closing_bracket {
                // Now handle indent level changes for the current paren level
                let current_paren = paren_stack.last_mut().unwrap();

                // Compare new indent with current indent levels using prefix matching
                // Pop levels where the new indent is not an extension of them
                while let Some(top) = current_paren.indent_stack.last() {
                    if indent_ws == top.whitespace {
                        // Same indent level
                        if top.in_indented {
                            // Same level but already had indented content = new block
                            // Pop and finalize
                            let finished = current_paren.indent_stack.pop().unwrap();
                            finalize_indent_level(current_paren, finished, pos);
                        } else {
                            // Continue in same block
                            break;
                        }
                    } else if indent_ws.starts_with(&top.whitespace) {
                        // New indent is deeper (extends current) - will push new level below
                        break;
                    } else if top.whitespace.starts_with(&indent_ws) {
                        // Dedenting - new indent is a prefix of current, pop this level
                        let finished = current_paren.indent_stack.pop().unwrap();
                        finalize_indent_level(current_paren, finished, pos);
                    } else {
                        // Neither is a prefix of the other - inconsistent indentation error
                        return Err(Error::new(
                            Span::new(indent_start, indent_ws.len().max(1)),
                            "Inconsistent indentation: whitespace pattern doesn't match previous levels",
                        ));
                    }
                }

                // Check if we need to create a new indent level
                if let Some(top) = current_paren.indent_stack.last_mut() {
                    if indent_ws.starts_with(&top.whitespace) && indent_ws != top.whitespace {
                        // Deeper indent - mark current as having indented content
                        top.in_indented = true;
                        current_paren.indent_stack.push(IndentLevel {
                            whitespace: indent_ws,
                            root_line: Vec::new(),
                            indented: Vec::new(),
                            in_indented: false,
                            span_start: pos,
                        });
                    }
                } else {
                    // No indent levels yet, create the first one
                    current_paren.indent_stack.push(IndentLevel {
                        whitespace: indent_ws,
                        root_line: Vec::new(),
                        indented: Vec::new(),
                        in_indented: false,
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

            // Opening brackets
            '(' | '[' | '{' => {
                let paren_type = ParenType::from_open(c);

                paren_stack.push(ParenLevel {
                    paren_type: Some(paren_type),
                    span_start: pos,
                    indent_stack: Vec::new(),
                    content: Vec::new(),
                });

                chars.next();
                pos += 1;
            }

            // Closing brackets
            ')' | ']' | '}' => {
                let expected_type = ParenType::from_close(c);

                // Check we're not trying to close the root
                if paren_stack.len() <= 1 {
                    return Err(Error::new(
                        Span::new(pos, 1),
                        format!("Unmatched closing bracket: '{}'", c),
                    ));
                }

                let mut entry = paren_stack.pop().unwrap();

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

                // Flush any remaining indents inside this paren
                entry.flush_indents(pos);

                // Create the Parens tokk
                let tokk = Tokk {
                    span: Span::from_range(entry.span_start, pos + 1),
                    content: TokkV::Parens(entry_type, entry.content),
                };

                // Add to parent's output
                let parent = paren_stack.last_mut().unwrap();
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
                let current = paren_stack.last_mut().unwrap();
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
                let current = paren_stack.last_mut().unwrap();
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
                    let current = paren_stack.last_mut().unwrap();
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
                    let current = paren_stack.last_mut().unwrap();
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
                    let current = paren_stack.last_mut().unwrap();
                    current.get_output().push(Toast::Tokk(tokk));
                }
            }
        }
    }

    // Check for unclosed parens (any beyond the root)
    if paren_stack.len() > 1 {
        let entry = &paren_stack[paren_stack.len() - 1];
        let paren_char = entry.paren_type.unwrap().close_char();
        return Err(Error::new(
            Span::new(entry.span_start, 1),
            format!("Unclosed bracket: '{}'", paren_char),
        ));
    }

    // Flush remaining indent stack in root and return its content
    let mut root = paren_stack.pop().unwrap();
    root.flush_indents(pos);
    Ok(root.content)
}

// Token Or Ast
// this isn't correct, you need modifications of Tokks and Asts that also link only to TokkOrAsts. Honestly you want a dynamically typed graph.

#[derive(Debug)]
enum Toast {
    Tokk(Tokk),
    Ast(Ast),
}
impl Toast {
    fn as_tokk(&self) -> &Tokk {
        match self {
            Toast::Tokk(tokk) => tokk,
            Toast::Ast(ast) => panic!("Toast is not a Tokk"),
        }
    }  
    fn as_ast(&self) -> &Ast {
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

/**
Takes a sequence of [Tokk]s and applies some rewrite rules to transform it into an Ast.
for each `for`, one of the following rules must match around its key term, and that rule transforms it into an Ast.
`%reverse` means it's greedy but from the other direction, processing terms from the right first

# indental belongs to the final term in the root line, never to the results of the operator

for o in operators:
    for o:
        %indental(%reverse($x* o $y*))($z*) → o($x %indental($y)($z*))
    for o:
        $x o $y → o($x $y)
        %indental(o $(x)?)($(y)*) → o($x $y)

for o in operators:
    for o:
        %indental($x o $y[where y contains no further operators] \n)($z*) → o($x %indental($y)($z*))

for "if", "else", "elif":
    if $c $x → if(condition($c) then($x))
    if $c $x $(elif $cel $xel)* else $y → if(condition($c) then($x) $(elif(condition($cel)) then($xel))... else($y))
    if($c $x $(elif $cel $xel)* else $y) → if(condition($c) then($x) $(elif(condition($cel)) then($xel))... else($y))
    if($c then($x*) elif($cel $xel)* else($y*)) → if(condition($c) then($x) $(elif(condition($cel)) then($xel))... else($y*))
    %indental(if $c)($x*) → if(condition($c) then($x*))
    
for "fn", "to":
    fn($parameters* to:$return $doings*) → fn(parameters($parameters*) returnType($return)) body($doings*))
    fn($parameters* to $doings*) → fn(parameters($parameters*) body($doings*))
    fn($parameters*) ($doings*) → fn(parameters($parameters*) body($doings*))
    %indental(fn $parameters*)($doings*) → fn(parameters($parameters*) body($doings*))
    %indental(fn $parameters* to)($doings*) → fn(parameters($parameters*) body($doings*))
    %indental(fn $parameters* to:$return)($doings*) → fn(parameters($parameters*) returnType($return) body($doings*))


*/
fn structure(mut tokks: Vec<Toast>, operators:&[String], operator_table: &OperatorTable, rules: &[KeywordRule]) -> Result<Ast, Error> {
    todo!("implement parse");
}

// struct Alteration<'a> {
//     remove: Vec<TastID>,
//     replacement: Box<Ast>,
// }

struct TastGraph {
    nodes: Vec<TastNode>,
}

struct TastCursor<'a> {
    sequence: &'a mut TastGraph,
    index: usize,
}

/// matches against any of the keywords
struct KeywordRule {
    keywords: Vec<String>,
    rule: fn(TastCursor<'a>) -> Result<(), Error>,
}

/// operators are ordered from highest to lowest precedence
fn parse(content: &str, operators:&[String], rules: &[KeywordRule]) -> Result<Ast, Error> {
    let operator_table = OperatorTable::new(operators.to_vec());
    let tokks = sequence(content, &operator_table)?;
    structure(tokks, operators, &operator_table, &rules)
}


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
    fn as_token(tokk: &Tokk) -> Option<&str> {
        match &tokk.content {
            TokkV::Token(s) => Some(s),
            _ => None,
        }
    }

    /// Helper to extract quoted string from tokk
    fn as_quoted(tokk: &Tokk) -> Option<(QuoteType, &str)> {
        match &tokk.content {
            TokkV::Quoted(qt, s) => Some((*qt, s)),
            _ => None,
        }
    }

    /// Helper to extract parens content from tokk
    fn as_parens(tokk: &Tokk) -> &Tokk {
        match &tokk.content {
            TokkV::Parens(pt, content) => content,
            _ => panic!("Toast is not a Parens"),
        }
    }

    /// Helper to extract indental from tokk
    fn as_indental(tokk: &Tokk) -> Option<(&Vec<Tokk>, &Vec<Tokk>)> {
        match &tokk.content {
            TokkV::Indental {
                root_line,
                indented,
            } => Some((root_line, indented)),
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
        assert_eq!(tokks.len(), 2);
        assert_eq!(as_token(&tokks[0]), Some("foo"));

        let (paren_type, content) = as_parens(&tokks[1]).expect("should be parens");
        assert!(matches!(paren_type, ParenType::Round));
        assert_eq!(content.len(), 1);
        assert_eq!(as_token(&content[0]), Some("bar"));
    }

    #[test]
    fn test_tokenize_square_brackets() {
        let ops = make_operator_table();
        let tokks = sequence("arr[0]", &ops).expect("should tokenize");
        assert_eq!(tokks.len(), 2);
        assert_eq!(as_token(&tokks[0]), Some("arr"));

        let (paren_type, content) = as_parens(tokks[1].as_tokk()).expect("should be parens");
        assert!(matches!(paren_type, ParenType::Square));
        assert_eq!(content.len(), 1);
        assert_eq!(as_token(&content[0]), Some("0"));
    }

    #[test]
    fn test_tokenize_curly_brackets() {
        let ops = make_operator_table();
        let tokks = sequence("{a b}", &ops).expect("should tokenize");
        assert_eq!(tokks.len(), 1);

        let (paren_type, content) = as_parens(tokks[0].as_tokk()).expect("should be parens");
        assert!(matches!(paren_type, ParenType::Curly));
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
        assert_eq!(scrips.len(), 2);
        assert_eq!(as_token(&scrips[0]), Some("f"));

        let (_, outer_content) = as_parens(&scrips[1]).expect("should be parens");
        assert_eq!(outer_content.len(), 2);
        assert_eq!(as_token(&outer_content[0]), Some("g"));

        let (_, inner_content) = as_parens(&outer_content[1]).expect("should be parens");
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
        // Items at the same indent level inside parens are flat
        let scrips = sequence("f(\n  a\n  b\n)", &ops).expect("should tokenize");

        assert_eq!(scrips.len(), 2);
        assert_eq!(as_token(&scrips[0]), Some("f"));

        let (_, content) = as_parens(&scrips[1]).expect("should be parens");
        // a and b at same indent level should be flat
        assert_eq!(content.len(), 2);
        assert_eq!(as_token(&content[0]), Some("a"));
        assert_eq!(as_token(&content[1]), Some("b"));
    }

    #[test]
    fn test_tokenize_indent_works_inside_parens() {
        let ops = make_operator_table();
        // Indentation creates structure inside parens too
        let scrips = sequence("f(\n  a\n    b\n)", &ops).expect("should tokenize");

        assert_eq!(scrips.len(), 2);
        assert_eq!(as_token(&scrips[0]), Some("f"));

        let (_, content) = as_parens(&scrips[1]).expect("should be parens");
        // a with indented b should form an Indental inside the parens
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
        // Indent structure resumes after paren closes
        let scrips = sequence("a\n  f(x)\n  b", &ops).expect("should tokenize");

        assert_eq!(scrips.len(), 1);
        let (root_line, indented) = as_indental(&scrips[0]).expect("should be indental");

        assert_eq!(as_token(&root_line[0]), Some("a"));
        
        // indented has: f, (x), b
        assert_eq!(indented.len(), 3);
        assert_eq!(as_token(&indented[0]), Some("f"));
        let (_, paren_content) = as_parens(&indented[1]).expect("should be parens");
        assert_eq!(as_token(&paren_content[0]), Some("x"));
        assert_eq!(as_token(&indented[2]), Some("b"));
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
        // Outside uses 2 spaces, inside paren uses tabs - allowed because paren resets indent
        let scrips = sequence("a\n  b(\n\tx\n\t\ty\n  )\n  c", &ops).expect("should tokenize");

        // Structure: a -> [b, (...), c] where inside paren is x -> [y]
        assert_eq!(scrips.len(), 1);
        let (root, indented) = as_indental(&scrips[0]).expect("outer indental");
        assert_eq!(as_token(&root[0]), Some("a"));

        // indented has: b, parens, c
        assert_eq!(indented.len(), 3);
        assert_eq!(as_token(&indented[0]), Some("b"));
        
        let (_, paren_content) = as_parens(&indented[1]).expect("should be parens");
        // Inside paren: x -> [y] using tab indentation
        assert_eq!(paren_content.len(), 1);
        let (inner_root, inner_indented) = as_indental(&paren_content[0]).expect("inner indental");
        assert_eq!(as_token(&inner_root[0]), Some("x"));
        assert_eq!(as_token(&inner_indented[0]), Some("y"));

        assert_eq!(as_token(&indented[2]), Some("c"));
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
}
