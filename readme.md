Attempts to make a programming language. Starting with The Perfect Syntax.

The project started when I thought I could see a way to spec The Perfect Syntax with a fairly simple step that converts a string into a token tree that captures indentation structure, then another quite simple step that converts everything the rest of the way using 25 rewrite rules, applied in order.

It turns out that may be true, but it's becoming clear that the rewrite rules are kind of ugly, and they fail to advise the programmer well as to how to apply them efficiently while reporting errors usefully. I could implement it as a series of rule applications, but it wouldn't be good, so I haven't.

The syntax has infix operators, negotiable whitespace structuring, and no need for semicolons or even commas, while still being totally intuitive.

The syntax might seem like it's not totally explicit about some things. This is good. Explicitness isn't the syntax's job any more. We've been in the age of LSP-integrated editors for like 7 years at this point. The syntax doesn't have to tell you whether your `=` expression is a definition or a reassignment, the editor can just read the code and show you which is which with an annotation. We also don't require explicit conversions (`.into()` and so on) because the editor can alert you whenever one's happening and tell you what it is if you ask.

```python
f = fn(a:int b:int to:int a + b)
print(f(1 2))
# > 3

# this is a comment
#(
  this is a multiline comment
)

combined = struct(a:int b:int)

fc = fn c:combined to int
    print("fc is being called")
    c.a + c.b

# there's also generally a paren form of these things, and when you enter parens it gets a bit more permissive about how you do indentation.
fb = fn(c:option[combined]
    to:int
    # todo, define if let syntax
    if c.is_some()
        c.a + c.b
    else 0)

c:option[combined] = some(combined(a = 2 b = 2))

# operators interact with indentation gracefully
ac = do
    g = fb
        combined
            a = 2
            b = 2
    +
        g
        3

# fully explicit paren modes syntax is also always possible
=(ac do(=(g fb(combined(=(a 2) =(b 2)))) +(g 3)))
```

Later on I'll be attempting type checking. Types with static evaluation (typechecking is staggered with interpretation), generics, value parameters, variance, maybe type inference but that's not what I want to think about just now, but it's going to need it if it is to be anything. And then garbage collection (note to self, go and see how Deno does it). And then wasm compilation. And then a very nice self-describing hash addressable object format. And then a database/OS and editor. And then some killer apps.