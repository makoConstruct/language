Attempts to make a programming language.

What's here so far, just a parser, however, it's the perfect parser. It consists entirely of a minimalistic conversion from raw text to structured tokens, and then just 15 rewrite rules which convert the parts into simple AST nodes. The final syntax has infix operators, negotiable whitespace structuring, and no need of semicolons or even commas.

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

