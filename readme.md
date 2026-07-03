Attempts to make a programming language. Starting with The Perfect Syntax.

Why was I doing this? I want a better web. A better web wants a shared language of types, a way of saying what a thing is, which then tells your databrowser/agent how to represent it to you and what can be done with it. There's a problem that arises in decentralized collaborations as a result of the fact that you may no longer be able to edit most of the types you're using, nor replace the types required, which leads to a need for a type system that is *collision-free*, meaning that any type can be combined with any other type. Which is to say, it needs to have ways of resolving ambiguity from member name collisions, and it needs to not have the diamond problem, and it may need to support a weird way of solving it where it can have two instances of the colliding supertype. No high level programming language's type system supports these things.

That doesn't mean they wouldn't be able to fairly frictionlessly use bindings to these types. But it means that there'd be no language for which the integration is direct and complete and beautiful. But also, if you define a language for types, which we need to do, then you're most of the way to having a programming language (the rest can mostly be imported from other programming languages or compiler chains)

So I want to make a programming language.

And I decided upon a syntax. But it was complex.

My enthusiasm for this syntax peaked when I thought it could be implemented entirely as a structuring step that converts a string into a token tree that captures indentation structure, then a step that just applies rewrite rules (25 of them) in order.

That may be feasible, but I'm not sure how to make the rewrite rules also communicate what's not allowed and report useful errors about it.

Instead I've attempted to just write the structurer as normal code. This was quite ugly, and I've decided to quit on it before it's done. I had no immediate intention of using the syntax, so we can camp for a bit and see if a paradigm for error reporting during matching comes to us.

But, the proposed/specced syntax has infix operators, negotiable whitespace structuring, and minimal punctuation. Commas are used sparingly as separators to prevent argument stealing (e.g., `a,(1 2)` to ensure `(1 2)` isn't parsed as `a(1 2)`). No semicolons needed. It's maximally readable, succinct, typeable.

The syntax might seem like it's not totally explicit about some things. But this is good. Explicitness isn't the syntax's job any more. We've been in the age of LSP-integrated editors for like 7 years at this point. The syntax doesn't have to tell you whether your `=` expression is a definition or a reassignment, the editor can just read the code and show you which it is with a visual hint. And we wont be explicit about conversions (`option[combined].into[combined]()` and so on) because the editor can flag whenever one's happening and tell you what it is if you click the flag.


```python
f = fn a:int b:int to:int a + b
print(f(1 2))
# > 3

# this is a comment
#(
  this is a multiline comment
)

combined = struct{ a:int b:int }

fc = fn c:combined to:int
    print("fc is being called")
    c.a + c.b

# there's also generally a paren form of these things, and when you enter parens it gets a bit more permissive about how you do indentation.
fb = fn(c:option[combined]
    to #return type doesn't always have to be given
    # todo, define if let syntax
    if c.is_some()
        c.a + c.b
    else 0)

c:option[combined] = some(combined(a = 2 b = 2))

# operators and functions interact with indentation gracefully
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

print
    list(1 2 3 4).map(fn a to tuple(1 a)).reduce
        tuple(0 0)
        fn tuple(a b) tuple(c d)
            a + b + c + d
# > 14
if a.all(to $a.isOdd())
    print("all {a.len} are odd")

# there'll probably be a whitespace function invocation syntax
print "hello" + " world"
# you can't nest it though (that makes things visually unclear)
print lowercase("HELLO WORLD")

# there's a do syntax that lets you define functions without writing a signature
print list(1 2 3 4).map(do $x + 2)
# > list(3 4 5 6)

```

Later on I'll be attempting type checking. Types with static evaluation (typechecking is staggered with interpretation), generics, value parameters, variance, maybe type inference but that's not what I want to think about just now, but it's going to need it if it is to be anything. And then garbage collection (note to self, go and see how Deno does it). And then wasm compilation. And then a very nice self-describing hash addressable object format. And then a database/OS and editor. And then some killer apps.
