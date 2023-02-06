use std::{
    any::Any,
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    sync::RwLock, rc::Rc,
};

pub trait QueryDef {
    const PATH: &'static str;
    type Input;
    type Output;
}

#[derive(Default)]
pub struct Database {
    fns: HashMap<&'static str, *const ()>,
    caches: RwLock<HashMap<&'static str, HashMap<u64, Rc<dyn Any + 'static>>>>,
    stack: RwLock<Vec<(&'static str, u64)>>,
}

#[derive(Debug)]
struct CachedComputation {
    version: usize,
    dependencies: Vec<(&'static str, u64)>,
    value: Rc<dyn Any + 'static>,
}

impl Database {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn register<Q>(&mut self, f: fn(&Self, Q::Input) -> Q::Output)
    where
        Q: QueryDef,
        Q::Output: 'static + Send + Sync,
    {
        let redefining = self.fns.insert(Q::PATH, f as *const ()).is_some();
        let mut caches = self.caches.write().unwrap();
        if redefining {
            let cache = caches
                .get_mut(Q::PATH)
                .expect("A query is missing its associated cache");
            for cc in cache.values_mut() {
                let cc_res = Rc::get_mut(cc).and_then(|c| c.downcast_mut());
                let cc: &mut CachedComputation = cc_res.unwrap();
                cc.version += 1;
                dbg!((Q::PATH, cc.version));
            }
        } else {
            caches.insert(Q::PATH, HashMap::new());
        }
    }

    pub fn run<I, O>(&self, q: &'static str, i: I) -> Rc<O>
    where
        I: Hash,
        O: 'static,
    {
        dbg!(q);
        let f = self.fns.get(q).expect("Unknown query");
        let f: fn(&Self, I) -> O = unsafe { std::mem::transmute(*f) };

        let mut hasher = DefaultHasher::new();
        i.hash(&mut hasher);
        let input_hash = hasher.finish();

        let old_version = {
            let caches = self.caches.read().unwrap();
            let cache = caches.get(q).expect("Unknown query cache");
            if let Some(c) = cache.get(&input_hash) {
                let c: Rc<CachedComputation> = c.clone().downcast().unwrap();
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
                dbg!(newest_dep);
                dbg!(c.version);
                if c.version >= newest_dep {
                    return c.value.clone().downcast().unwrap();
                } else {
                    newest_dep
                }
            } else {
                0
            }
        };

        {
            let mut caches = self.caches.write().unwrap();
            let cache = caches.get_mut(q).expect("Unknown query cache");
            let cc = Rc::new(CachedComputation {
                version: old_version + 1,
                dependencies: vec![],
                value: Rc::new(()),
            });
            cache.insert(input_hash, cc);
        };

        {
            let mut stack = self.stack.write().unwrap();
            let stack_top = stack.iter().last().cloned();
            stack.push((q, input_hash));

            if let Some(stack_top) = dbg!(stack_top) {
                let mut caches = self.caches.write().unwrap();
                let cache = caches.get_mut(stack_top.0).unwrap();
                let cc = cache.get_mut(&stack_top.1).unwrap();
                let cc: &mut CachedComputation = Rc::get_mut(cc).unwrap().downcast_mut().unwrap();
                cc.dependencies.push((q, input_hash));
            }
        };

        let out = f(&self, i);

        {
            let stack = self.stack.write();
            stack.unwrap().pop();
        }

        {
            let mut caches = self.caches.write().unwrap();
            let cache = caches.get_mut(q).unwrap();
            let cc = cache.get_mut(&input_hash).unwrap();
            let cc: &mut CachedComputation = Rc::get_mut(cc).unwrap().downcast_mut().unwrap();
            cc.value = Rc::new(out);
            cc.value.clone().downcast().unwrap()
        }
    }
}

#[macro_export]
macro_rules! query {
    ($name:ident, $i:ty, $o:ty) => {
        pub mod $name {
            #[allow(non_camel_case_types)]
            pub struct Query;

            impl yeter::QueryDef for Query {
                const PATH: &'static str = stringify!($name);
                type Input = $i;
                type Output = $o;
            }

            pub fn query(db: &yeter::Database, i: $i) -> std::rc::Rc<$o> {
                use yeter::QueryDef;
                db.run::<$i, $o>(Query::PATH, i)
            }
        }
    };
}

#[macro_export]
macro_rules! queries {
    ($m:expr, $name:ident, $i:ty, $o:ty) => {
        pub mod $name {
            #[allow(non_camel_case_types)]
            pub struct Query;

            impl yeter::QueryDef for Query {
                const PATH: &'static str = concat!($m, "/", stringify!($name));
                type Input = $i;
                type Output = $o;
            }

            pub fn query(db: &yeter::Database, i: $i) -> std::rc::Rc<$o> {
                use yeter::QueryDef;
                db.run::<$i, $o>(Query::PATH, i)
            }
        }
    };
    ($mname:ident {
        $( $name:ident : $i:ty : $o:ty ),*
    }) => {
        pub mod $mname {
            $( yeter::queries! { stringify!($mname), $name, $i, $o } )*
        }
    };
    ($mname:ident {
        $( $name:ident : $i:ty : $o:ty ),*
    }, $( $rest:tt )*) => {
        pub mod $mname {
            $( yeter::queries! { stringify!($mname), $name, $i, $o } )*
        }

        yeter::queries! {
            $( $rest )*
        }
    }
}
