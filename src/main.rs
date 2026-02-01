// the final product of computation is a graph of executable expressions, represented in an arena of refs. The compilation process is mostly about checking the types and converting the text into a graph.

//parser: first it just paren match, then use the matched parens to parse into a Vec of Asts

use std::{any::Any, collections::HashMap};
mod parser;
use crate::parser::{Ast, Error, Span};

use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

/// refers to something in the Executable graph
pub struct Ref<T: ?Sized = dyn Any>(pub u64, PhantomData<fn() -> *const T>);

impl<T: ?Sized> Clone for Ref<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> Copy for Ref<T> {}

impl<T: ?Sized> fmt::Debug for Ref<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Ref").field(&self.0).finish()
    }
}

impl<T: ?Sized> PartialEq for Ref<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: ?Sized> Eq for Ref<T> {}

impl<T: ?Sized> Hash for Ref<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<T: ?Sized> Ref<T> {
    pub fn erase(self) -> Ref<dyn Any> {
        Ref(self.0, PhantomData)
    }
}



/// a frontier at which interpretation is being done, but will pause whenever it encounters a reference to a value that is not yet defined.
struct Frontier {
    r: Ref,
}

/// it's not efficient or anything, it's just a simple map for representing possibly graphic data.
struct Arena {
    values: HashMap<Ref<dyn Any>, Box<dyn Any>>,
}
impl Arena {
    fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }
    fn get<T: 'static>(&self, r: Ref<T>) -> Result<&T, Error> {
        self.values
            .get(&r.erase())
            .ok_or(Error::new(
                Span::new(0, 0),
                format!("Reference {:?} not found", r),
            ))
            .and_then(|b| {
                b.downcast_ref::<T>().ok_or(Error::new(
                    Span::new(0, 0),
                    "Type mismatch when dereferencing".to_string(),
                ))
            })
    }
    fn create<T: 'static>(&mut self, value: T) -> Ref {
        let r: Ref = Ref(self.values.len() as u64, PhantomData);
        self.values.insert(r.clone(), Box::new(value));
        r
    }
    fn remove(&mut self, r: Ref)-> Option<Box<dyn Any>> {
        self.values.remove(&r)
    }
}

/// I think I want this to be typed objects? Idk, why? The compiler/interpreter doesn't need them to be, but you want to distribute this as a kind of well defined binary format.
/// so this can be thought of as a reduced shadow of a typed object graph format.
enum CodeV {
    Invocation { on: Ref, args: Vec<Ref> },
    Function { args: Vec<Ref>, body: Ref },
    Def { expression: Ref },
    Const(Vec<u8>),
    BuiltinInvocation { code: u64, args: Vec<Ref> },
}

struct Code {
    file: String,
    span: Span,
    code: CodeV,
}

struct Compiler {
    fronts: HashMap<String, Frontier>,
    arena: Arena,
}
impl Compiler {
    fn new() -> Self {
        Self {
            fronts: HashMap::new(),
            arena: Arena::new(),
        }
    }
    fn compile(self, _source: Ast) -> (Code, Arena) {
        todo!("implement compilation")
    }
}

fn main() {
    println!("Hello, world!");
}
