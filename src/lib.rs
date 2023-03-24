mod ns_type_id;

use ns_type_id::NsTypeId;
use std::{
    any::Any,
    cell::RefCell,
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    rc::Rc,
    sync::RwLock,
};

use state::Container;

/// A query definition
///
/// Implementations can be created with [`#[yeter::query]`][query].
pub trait QueryDef {
    /// Input type
    type Input;
    /// Output type
    type Output;
}

/// A query definition for an _input query_
///
/// Implementations can be created with [`#[yeter::query]`][query] on a function with no body.
///
/// An _input query_ can be assigned a value using [`Database::set`].
pub trait InputQueryDef: QueryDef<Output = Option<Self::OptionalOutput>> {
    type OptionalOutput;
}

type RcAny = Rc<dyn Any + 'static>;

/// The main type to interact with Yéter
///
/// This structure holds _caches_ and _effects_ for each query type.
#[derive(Default)]
pub struct Database {
    /// The caches
    ///
    /// It associates a query name with its cache.
    /// A query cache associates an input hash with the corresponding output.
    caches: RwLock<HashMap<NsTypeId, HashMap<u64, CachedComputation>>>,
    /// Current call stack, to track dependencies
    stack: RwLock<Vec<(NsTypeId, u64)>>,
    /// Effects that have been executed by the current query
    effects: RwLock<state::Container![Send]>,
}

/// A cache item
#[derive(Debug)]
struct CachedComputation {
    /// The version of this item (starts at 1 and goes up with every recomputation)
    version: usize,
    /// The other query calls this computation depends on
    dependencies: Vec<(NsTypeId, u64)>,
    /// The output
    value: RcAny,
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
        matches!(
            self.value.downcast_ref::<UninitCachedComputationValue>(),
            Some(UninitCachedComputationValue)
        )
    }
}

impl Database {
    /// Creates an empty database
    pub fn new() -> Self {
        Default::default()
    }

    /// Runs a query (or not if it the result is already in the cache)
    ///
    /// Panics if a query ends up in a cyclic computation
    pub fn run<'input, F, Q>(&self, f: F, i: Q::Input) -> Rc<Q::Output>
    where
        F: Fn(&Database, Q::Input) -> Q::Output,
        Q: QueryDef,
        Q::Input: Hash + 'input,
        Q::Output: 'static,
    {
        self.try_run::<F, Q>(f, i).unwrap()
    }

    /// Tries to runs a query (or not if it the result is already in the cache)
    pub fn try_run<'input, F, Q>(&self, f: F, i: Q::Input) -> Result<Rc<Q::Output>, CycleError>
    where
        F: Fn(&Database, Q::Input) -> Q::Output,
        Q: QueryDef,
        Q::Input: Hash + 'input,
        Q::Output: 'static,
    {
        let q = NsTypeId::of::<Q>();

        let mut hasher = DefaultHasher::new();
        i.hash(&mut hasher);
        let input_hash = hasher.finish();

        let old_version = {
            let caches = self.caches.read().unwrap();
            let cache = caches.get(&q);
            if let Some(c) = cache.and_then(|c| c.get(&input_hash)) {
                if !c.redefined {
                    let newest_dep = c
                        .dependencies
                        .iter()
                        .map(|(f, k)| {
                            caches
                                .get(f)
                                .expect("Uknown query (dependency of another query)")
                                .get(k)
                                .expect("A cached computation has a non-cached dependency")
                                .version
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
            let cc = CachedComputation::new(old_version + 1);
            let cache = caches.entry(q).or_default();
            cache.insert(input_hash, cc);
        };

        {
            let mut stack = self.stack.write().unwrap();
            let stack_top = stack.iter().last().cloned();
            stack.push((q, input_hash));

            if let Some(stack_top) = stack_top {
                let mut caches = self.caches.write().unwrap();
                let cache = caches.get_mut(&stack_top.0).unwrap();
                let cc = cache.get_mut(&stack_top.1).unwrap();
                cc.dependencies.push((q, input_hash));
            }
        };

        let out = Rc::new(f(self, i));

        {
            let stack = self.stack.write();
            stack.unwrap().pop();
        }

        {
            let mut effects = self.effects.write().unwrap();
            let effects = std::mem::replace(&mut *effects, <Container![Send]>::new());

            let mut caches = self.caches.write().unwrap();
            let cache = caches.get_mut(&q).unwrap();
            let cc = cache.get_mut(&input_hash).unwrap();
            cc.effects = effects;
            cc.value = out;
            cc.value
                .clone()
                .downcast()
                .map(Ok)
                .expect("Cached computation was not of the correct type")
        }
    }

    /// Returns a side effect collection
    pub fn effect<'a, T: 'static + Clone>(&'a self) -> Vec<T> {
        let caches = self.caches.read().unwrap();
        caches
            .values()
            .flat_map(|x| x.values())
            .filter_map(|cc| {
                let cell = cc.effects.try_get::<RefCell<Vec<T>>>()?.borrow();
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

    /// Defines the a value
    pub fn set<'input, Q>(&self, input: Q::Input, output: Q::Output)
    where
        Q: InputQueryDef,
        Q::Input: Hash + 'input,
        Q::OptionalOutput: 'static,
    {
        let q = NsTypeId::of::<Q>();

        let mut hasher = DefaultHasher::new();
        input.hash(&mut hasher);
        let input_hash = hasher.finish();

        let output: Rc<dyn Any> = Rc::new(output);

        let default_cc = CachedComputation {
            version: 1,
            dependencies: Vec::new(),
            value: Rc::clone(&output),
            effects: <state::Container![Send]>::new(),
            redefined: false,
        };

        let mut caches = self.caches.write().unwrap();
        let cache = caches.entry(q).or_default();
        let cc = cache.entry(input_hash).or_insert(default_cc);
        cc.value = output;
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
/// used to uniquely identify a given query. If the function is declared without a body, the query
/// is considered an _input query_; it must return an [Option] and its return value for a given input
/// can be set in an imperative way using [`Database::set`].
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
/// // Usage
/// # fn main() {
/// let db = yeter::Database::new();
/// println!("{}", length(&db, "hello world".into()));
/// # }
/// ```
///
/// It is also possible to declare an **input query** by omitting the body. Input queries must
/// return [Option]s and upon invocation, will return [`None`] by default. They can be assigned a
/// value with [`Database::set`].
///
/// ```
/// # use std::path::PathBuf;
/// // Declaration of an input query
/// #[yeter::query]
/// fn all_workspace_files(db: &yeter::Database) -> Option<Vec<PathBuf>>;
///
/// // Definition of its value
/// # fn main() {
/// let mut db = yeter::Database::new();
/// db.set::<all_workspace_files>((), Some(vec![ /* ... */ ]));
/// # }
/// ```
pub use yeter_macros::query;
