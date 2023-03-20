use std::{
    any::Any,
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    sync::RwLock, rc::Rc, marker::PhantomData, cell::RefCell,
};

use state::Container;

struct ConstrainedFn<A, O, F: Fn(&Database, A) -> O> {
    f: F,
    _arg: PhantomData<A>,
    _out: PhantomData<O>,
}

trait AnyFn {
    fn try_call(&self, db: &Database, arg: Box<dyn Any>) -> Option<Rc<dyn Any>>;
}

impl<A: 'static, O: 'static, F: Fn(&Database, A) -> O + 'static> AnyFn for ConstrainedFn<A, O, F> {
    fn try_call(&self, db: &Database, arg: Box<dyn Any>) -> Option<Rc<dyn Any>> {
        if let Ok(arg) = arg.downcast() {
            Some(Rc::new((self.f)(db, *arg)))
        } else {
            None
        }
    }
}


/// A query definition
///
/// Implementations can be created with [`#[yeter::query]`][query] and can be registered with
/// [`Database::register`].
pub trait QueryDef {
    /// The path of the query
    const PATH: &'static str;
    /// Input type
    type Input;
    /// Output type
    type Output;
}

/// A query definition with an implicit definition
///
/// Implementations can be created with [`#[yeter::query]`][query] and can be registered with
/// [`Database::register_impl`].
pub trait ImplementedQueryDef: QueryDef {
    fn run(db: &Database, input: Self::Input) -> Self::Output;
}

/// The main type to interact with Yéter
///
/// This structure holds a list of registered queries and their respective caches.
#[derive(Default)]
pub struct Database {
    /// Registered queries
    fns: HashMap<&'static str, Box<dyn AnyFn>>,
    /// The caches
    ///
    /// It associates a query name with its cache.
    /// A query cache associates an input hash with the corresponding output.
    caches: RwLock<HashMap<&'static str, HashMap<u64, Rc<dyn Any + 'static>>>>,
    /// Current call stack, to track dependencies
    stack: RwLock<Vec<(&'static str, u64)>>,
    /// Effects that have been executed by the current query
    effects: RwLock<state::Container![Send]>,
}

/// A cache item
#[derive(Debug)]
struct CachedComputation {
    /// The version of this item (starts at 1 and goes up with every recomputation)
    version: usize,
    /// The other query calls this computation depends on
    dependencies: Vec<(&'static str, u64)>,
    /// The output
    value: Rc<dyn Any + 'static>,
    /// Saved side effects
    effects: state::Container![Send],
    /// Wheter or not the associated query was redefined. If true, this cache item
    /// is invalid and should be recomputed.
    redefined: bool,
}

/// Error returned by [Database::try_run] when a 
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CycleError;

struct UninitCachedComputationValue;

impl CachedComputation {
    fn new(version: usize) -> Self {
        CachedComputation {
            version,
            dependencies: vec![],
            value: Rc::new(UninitCachedComputationValue),
            redefined: false,
            effects: <state::Container![Send]>::new(),
        }
    }

    fn is_uninit(&self) -> bool {
        matches!(self.value.downcast_ref::<UninitCachedComputationValue>(), Some(UninitCachedComputationValue))
    }
}

impl Database {
    /// Creates an empty database
    pub fn new() -> Self {
        Default::default()
    }

    /// Registers a query
    ///
    /// Refer to the [`#[yeter::query]`][query] macro's documentation for help with creating a
    /// query.
    ///
    /// This function is idempotent, and a query may be redefined multiple times.
    ///
    /// # Note
    ///
    /// Use [Database::register_impl] to register queries with an implicit definition. The
    /// [register][Database::register] method requires you to provide a definition and will
    /// completely ignore any implicit one.
    pub fn register<F, Q>(&mut self, f: F)
    where
        F: Fn(&Self, Q::Input) -> Q::Output + 'static,
        Q: QueryDef,
        Q::Input: 'static,
        Q::Output: 'static,
    {
        let redefining = self.fns.insert(Q::PATH, Box::new(ConstrainedFn {
            f: Box::new(f),
            _arg: PhantomData,
            _out: PhantomData,
        })).is_some();
        let mut caches = self.caches.write().unwrap();
        if redefining {
            let cache = caches
                .get_mut(Q::PATH)
                .expect("A query is missing its associated cache");
            for cc in cache.values_mut() {
                let cc_res = Rc::get_mut(cc).and_then(|c| c.downcast_mut());
                let cc: &mut CachedComputation = cc_res.unwrap();
                cc.version += 1;
                cc.redefined = true;
            }
        } else {
            caches.insert(Q::PATH, HashMap::new());
        }
    }

    /// Registers a query that has an implicit definition
    ///
    /// Refer to the [`#[yeter::query]`][query] macro's documentation for help with creating an
    /// implicitly-defined query.
    ///
    /// This function directly calls [Database::register]. Therefore, it is also idempotent, and any
    /// query registered with it can be later manually overridden with [Database::register].
    pub fn register_impl<Q>(&mut self)
    where
        Q: ImplementedQueryDef + 'static,
        Q::Input: 'static,
        Q::Output: 'static,
    {
        self.register::<_, Q>(Q::run)
    }

    /// Runs a query (or not if it the result is already in the cache)
    /// 
    /// Panics if a query ends up in a cyclic computation
    pub fn run<I, O>(&self, q: &'static str, i: I) -> Rc<O>
        where
            I: Hash + 'static,
            O: 'static,
    {
        self.try_run(q, i).unwrap()
    }

    /// Tries to runs a query (or not if it the result is already in the cache)
    pub fn try_run<I, O>(&self, q: &'static str, i: I) -> Result<Rc<O>, CycleError>
    where
        I: Hash + 'static,
        O: 'static,
    {
        let f = self.fns.get(q).expect("Unknown query");

        let mut hasher = DefaultHasher::new();
        i.hash(&mut hasher);
        let input_hash = hasher.finish();

        let old_version = {
            let caches = self.caches.read().unwrap();
            let cache = caches.get(q).expect("Unknown query cache");
            if let Some(c) = cache.get(&input_hash) {
                let c: Rc<CachedComputation> = c.clone().downcast().unwrap();
                if !c.redefined {
                    let newest_dep = c
                        .dependencies
                        .iter()
                        .map(|(f, k)| {
                            let dep: Rc<CachedComputation> = caches
                                .get(f)
                                .expect("Uknown query (dependency of another query)")
                                .get(k)
                                .expect("A cached computation has a non-cached dependency")
                                .clone()
                                .downcast()
                                .unwrap();
                            dep.version
                        })
                        .max()
                        .unwrap_or(1);
                    if c.version >= newest_dep {
                        if let Ok(v) = c.value.clone().downcast() {
                            return Ok(v);
                        } else if c.is_uninit() {
                            return Err(CycleError);
                        } else {
                            unimplemented!("impossible downcast")
                        }
                    } else {
                        newest_dep
                    }
                } else {
                    c.version
                }
            } else {
                0
            }
        };

        {
            let mut caches = self.caches.write().unwrap();
            let cache = caches.get_mut(q).expect("Unknown query cache");
            let cc = Rc::new(CachedComputation::new(old_version + 1));
            cache.insert(input_hash, cc);
        };

        {
            let mut stack = self.stack.write().unwrap();
            let stack_top = stack.iter().last().cloned();
            stack.push((q, input_hash));

            if let Some(stack_top) = stack_top {
                let mut caches = self.caches.write().unwrap();
                let cache = caches.get_mut(stack_top.0).unwrap();
                let cc = cache.get_mut(&stack_top.1).unwrap();
                let cc: &mut CachedComputation = Rc::get_mut(cc).unwrap().downcast_mut().unwrap();
                cc.dependencies.push((q, input_hash));
            }
        };

        let out = f.try_call(&self, Box::new(i)).unwrap();

        {
            let stack = self.stack.write();
            stack.unwrap().pop();
        }

        {
            let mut effects = self.effects.write().unwrap();
            let effects = std::mem::replace(&mut *effects, <Container![Send]>::new());

            let mut caches = self.caches.write().unwrap();
            let cache = caches.get_mut(q).unwrap();
            let cc = cache.get_mut(&input_hash).unwrap();
            let cc: &mut CachedComputation = Rc::get_mut(cc).unwrap().downcast_mut().unwrap();
            cc.effects = effects;
            cc.value = out;
            cc.value.clone().downcast().map(Ok).expect("Cached computation was not of the correct type")
        }
    }

    /// Returns a side effect collection
    pub fn effect<'a, T: 'static + Clone>(&'a self) -> Vec<T> {
        let caches = self.caches.read().unwrap();
        caches.values()
            .flat_map(|x| x.values())
            .filter_map(|x| {
                let cc: Rc<CachedComputation> = Rc::clone(x).downcast().expect("Database::effect: invalid cache");
                let cell  = cc.effects.try_get::<RefCell<Vec<T>>>()?.borrow();
                Some(cell.clone())
            })
            .flatten()
            .collect()
    }

    /// Produces a side effect
    pub fn do_effect<T: 'static + Clone + Send>(&self, eff: T) {
        let effects = self.effects.write().unwrap();
        if effects.try_get::<RefCell<Vec<T>>>().is_none() {
            effects.set::<RefCell<Vec<T>>>(RefCell::new(Vec::new()));
        }
        let vec = effects.get::<RefCell<Vec<T>>>();
        let mut vec = vec.borrow_mut();
        vec.push(eff);
    }
}

/// Annotates a function to make it a _query_ that benefits from Yéter's features
///
/// # Usage
///
/// When annotated, a function will be turned into a Yéter _query_. Its return value is turned into
/// an [`Rc<T>`][std::rc::Rc] where `T` is the original declared return value. Calls to query
/// functions will benefit from Yéter's memoization and side effect system.
///
/// In addition to the modified function, the macro also produces a type-level empty enum that is
/// used to uniquely identify a given query. If the function is declared with a body, an additional
/// "implicit definition" will be attached to this type, and it will be possible to register
/// the query definition and declaration simultaneously with [Database::register_impl].
///
/// # Syntax
///
/// `#[yeter::query]` doesn't expect any attribute parameters. It must be applied to a function with
/// or without a body, whose first argument is present and is typed as a
/// [`&yeter::Database`][Database]. The function cannot be an instance method (i.e. have a `self`
/// receiver as its first argument).
///
/// # Example
///
/// ```
/// // Declaration and implicit definition
/// #[yeter::query]
/// fn length(db: &yeter::Database, input: String) -> usize {
///     input.len()
/// }
///
/// // Registration
/// # fn main() {
/// let mut db = yeter::Database::new();
/// db.register_impl::<length>();
/// # }
/// ```
///
/// It is also possible to declare a query and define it later. Trying to use it before registration
/// causes a runtime error.
///
/// ```
/// # use std::path::PathBuf;
/// // Declaration only
/// #[yeter::query]
/// fn all_workspace_files(db: &yeter::Database) -> Vec<PathBuf>;
///
/// // Definition and registration
/// # fn main() {
/// let mut db = yeter::Database::new();
/// db.register::<_, all_workspace_files>(|db, ()| {
///     vec![ /* ... */ ]
/// });
/// # }
/// ```
///
/// # See also
///
///   * [`Database::register_impl`] to register a query that has an _implicit definition_
///   * [`Database::register`] to both register a query and define it
pub use yeter_macros::query;
