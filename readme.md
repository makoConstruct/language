Attempts to make a programming language. Starting with The Perfect Syntax.

The project started when I thought I could see a way to spec The Perfect Syntax with a fairly simple step that converts a string into a token tree that captures indentation structure, then another quite simple step that converts everything the rest of the way using 25 rewrite rules, applied in order.

It turns out that may be true, but it's becoming clear that the rewrite rules are kind of ugly, and they fail to advise the programmer well as to how to apply them efficiently while reporting errors usefully.

So, here is The Perfect Syntax (spec). It has infix operators, negotiable whitespace structuring, and no need for semicolons or even commas, while still being totally intuitive.

I'm not sure when I'll implement it. But it wouldn't take long given that we already have the rules and it could be done just as a series of rule applications (but shouldn't, it would be totally crap at providing explanations for syntax errors).

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

# there's also generally a paren form of these things, and when you enter parens it gets a bit more permissive about how you do indentation.
fb = fn(c:combined
    to:int
    c.a + c.b)

c:option[combined] = some(combined(a = 2 b = 2))

ac =
    if c
        print("it was a combined")
        c.a + c.b
    else 0

g = +(ac fb(c) 9) * 2
```

Later on I'll be attempting type checking. Types with static evaluation (typechecking is staggered with interpretation), generics, value parameters, variance, maybe type inference but that's not what I want to think about just now, but it's going to need it if it is to be anything. And then garbage collection (note to self, go and see how Deno does it). And then wasm compilation. And then a very nice self-describing hash addressable object format. And then a database/OS and editor. And then some killer apps.