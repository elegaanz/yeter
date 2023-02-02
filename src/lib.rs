use std::{collections::{HashMap, hash_map::DefaultHasher}, sync::{RwLock, Arc}, any::{Any,}, hash::{Hash, Hasher}};

pub trait QueryDef {
    const PATH: &'static str;
    type Input;
    type Output;
}

pub struct Database {
    fns: HashMap<&'static str, *const ()>,
    caches: RwLock<HashMap<&'static str, HashMap<u64, Arc<dyn Any + Send + Sync>>>>,
}

struct CachedComputation<T> {
    version: usize,
    dependencies: Vec<(&'static str, u64)>,
    value: Arc<T>,
}

impl Database {
    pub fn new() -> Self {
        Self {
            fns: Default::default(),
            caches: Default::default(),
        }
    }

    pub fn register<Q,>(&mut self, f: fn(Q::Input) -> Q::Output)
    where Q: QueryDef {
        self.fns.insert(Q::PATH, f as *const ());
        let mut caches = self.caches.write().unwrap();
        caches.insert(Q::PATH, HashMap::new());
    }

    pub fn run<I, O>(&self, q: &'static str, i: I) -> Arc<O> where I: Hash, O: 'static + Send + Sync {
        let f = self.fns.get(q).expect("Unknown query");
        let f: fn(I) -> O = unsafe { std::mem::transmute(*f) };

        let mut hasher = DefaultHasher::new();
        i.hash(&mut hasher);
        let i_hash = hasher.finish();

        {
            let caches = self.caches.read().unwrap();
            let cache = caches.get(q).expect("Unknown query cache");
            if let Some(c) = cache.get(&i_hash) {
                let c: Arc<CachedComputation<O>> = c.clone().downcast().unwrap();
                let newest_dep = c.dependencies.iter()
                    .map(|(f, k)| {
                        // I didn't check but it is probably not possible to use
                        // proper downcasting here, since the output of other queries
                        // may not be O
                        // This is safe since we only read version (if we consider that
                        // fields are not reordered by the compiler......................)
                        // TL;DR: this is probably unsafe, don't look too close, I'm prototyping
                        let dep = caches.get(f)
                            .expect("Uknown query (dependency of another query)")
                            .get(k)
                            .expect("A cached computation has a non-cached dependency");
                        let dep: &Box<CachedComputation<O>> = unsafe { std::mem::transmute(dep) };
                        dep.version
                    })
                    .max()
                    .unwrap_or_default();
                if c.version >= newest_dep {
                    return c.value.clone();
                }
            }
        }

        let mut caches = self.caches.write().unwrap();
        let cache = caches.get_mut(q).expect("Unknown query cache");
        let out = f(i);
        cache.insert(i_hash, Arc::new(CachedComputation {
            version: 0,
            dependencies: vec![],
            value: Arc::new(out),
        }));
        let arc_arc = cache.get(&i_hash).unwrap().clone().downcast::<CachedComputation<O>>().unwrap();
        arc_arc.value.clone()
    }
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

            pub fn query(db: &yeter::Database, i: $i) -> std::sync::Arc<$o> {
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
