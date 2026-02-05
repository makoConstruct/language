Attempts to make a programming language.

What's here so far, just a parser, however, it's the perfect parser. It consists entirely of a minimalistic conversion from raw text to structured tokens, and then just 16 rewrite rules in mostly one pass which convert the parts into simple AST nodes.

The default syntax ("language") has infix operators, negotiable whitespace structuring, and no need of semicolons or even commas.

The syntax is like this:

```
f = fn(a:int b:int to:int a + b)
print(f(1 2))
# this is a comment
#(
  this is a multiline comment
)
combined = struct(a:int b:int)


fc = fn c:combined to int
    print("fc is being called")
    c.a + c.b

# there's also generally a paren form of these things.
fb = fn(c:combined
    to:int
c.a + c.b)

ac =
    if c
        print("it was a combined")
        c.a + c.b
    else 0
```

Later on I'll be attempting type checking. Types with with full static evaluation, generics, value parameters, variance, maybe type inference but that's not what I want to think about just now, but it's going to need it if it is to be anything.

And then garbage collection, note to self, go and see how Deno does it.

And then wasm compilation.

And then a very nice self-describing hash addressable object format.

And then a database/OS and editor.

And then some killer apps.