Attempts to make a programming language. Starting with The Perfect Syntax.

My enthusiasm for this peaked when I thought it could be implemented entirely as a structuring step that converts a string into a token tree that captures indentation structure, then a step that just applies rewrite rules (25 of them) in order.

That may be feasible, but it's becoming clear that the rewrite rules are kind of ugly, and they fail to advise the programmer in how to apply them efficiently, while reporting errors usefully.

But, the proposed/specced syntax has infix operators, negotiable whitespace structuring, and no need for semicolons or even commas. It's maximally readable, succinct, typeable.

The syntax might seem like it's not totally explicit about some things. But this is good. Explicitness isn't the syntax's job any more. We've been in the age of LSP-integrated editors for like 7 years at this point. The syntax doesn't have to tell you whether your `=` expression is a definition or a reassignment, the editor can just read the code and show you which it is with a visual hint. And we wont be explicit about conversions (`option[combined].into[combined]()` and so on) because the editor can flag whenever one's happening and tell you what it is if you click the flag.

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

# we don't have default lists and maps and so on, all datastructures are constructed in the normal way, by naming them
print
    list(1 2 3 4).map(fn a to tuple(1 a)).reduce
        tuple(0 0)
        fn tuple(a b) tuple(c d)
            a + b + c + d
# > 14
```

Later on I'll be attempting type checking. Types with static evaluation (typechecking is staggered with interpretation), generics, value parameters, variance, maybe type inference but that's not what I want to think about just now, but it's going to need it if it is to be anything. And then garbage collection (note to self, go and see how Deno does it). And then wasm compilation. And then a very nice self-describing hash addressable object format. And then a database/OS and editor. And then some killer apps.