Attempts to make a programming language. Starting with The Perfect Syntax.

The project started when I thought I could see a way to spec The Perfect Syntax with a fairly simple step that converts a string into a token tree that captures indentation structure, then another quite simple step that converts everything the rest of the way using about 20 rewrite rules.

This turned out not to be the case, or at least, I couldn't find the rewrite rules that would do it. I'm stumped mainly on

```
a + if c d else b + c
```
needing to parse to
```
a + if(c d else(b + c))
```
In some sense we need operator rules to have higher precedence than if else rules so that the end comes through as `else(b+c)` instead of `else(b) + c` (and this is even more critical for situations like `if c && c2 d + e else ...`) but also we want them to be lower precedence so that the begining comes out as `a + if(c ...` instead of `(a + if) c ...`. So we're paradoxed.

We could also just do away with inline ternary if and only allow `if(c d) else(e)` and give if else sequencing higher precedence than operators.

But once I find the rewrite rules, or digest the inelegant reality of having to do this with something much uglier than rewrite rules, we will have The Perfect Syntax. it will have infix operators, negotiable whitespace structuring, and no need for semicolons or even commas.

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




Later on I'll be attempting type checking. Types with with full static evaluation, generics, value parameters, variance, maybe type inference but that's not what I want to think about just now, but it's going to need it if it is to be anything. And then garbage collection, note to self, go and see how Deno does it. And then wasm compilation. And then a very nice self-describing hash addressable object format. And then a database/OS and editor. And then some killer apps.