Attempts to make a programming language.

The most recent run involved an attempt to implement The Perfect Syntax:

```
f = fn(a:int b:int to:int a + b)
print(f(1 2))
# this is a comment
#(
  this is a multiline comment
)
combined = struct(a:int b:int)

fb = fn(c:combined to:int c.a + c.b)
fc = fn c:combined to int
    c.a + c.b

ac =
    if c
        c.a + c.b
    else 0
```

This time the main insight was first converting the raw text into a tree of
```
pub enum Tokk {
    Token(String),
    Quoted(QuoteType, String),
    Parens(ParenType, Vec<Tokk>),
    Indental {
        head_line: Vec<Tokk>,
        indented: Vec<Tokk>,
    },
}
```
s, (and that part seems to work fine, and is hopeful, and good)

Before then applying reducer rules to convert all of the infix operators and special cases for indentals into a normal ast.

I didn't quite close it before deciding this wasn't productive and moving on. Maybe next time. I kinda choked on

```
fc = fn c:combined to int
    c.a + c.b
```

Where indental under operators should yield the indental to the final term. But only while it's infix. The transform rule seems obvious but I just couldn't see where to fit the rule into the rule precedence.

Regardless, that's enough for now.