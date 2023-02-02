# Yéter — KISS incremental computation framework

```rust
let mut driver = Driver::new();
driver.add_file("main.lus");
let ty = driver.type_check("toutpareil")?;

queries! {
    typing {
        check {
            node: String -> Option<Type>
        }
    }
};

mod queries {
    mod typing {
        mod check {
            struct node;
            impl Query for node {
                const PATH: &'static str = "typing/check/node";
                type Input = String;
                type Output = Option<Type>;
            }
        }
    }
}

let mut db = yeter::Database::new();
db.register(queries::typing::check::node, |key: String| {

});
db.run(queries::typing::check::node, "toutpareil")
typing::check::node(db, "toutpareil");

register(q, f) {
    self.fns.insert(q::PATH, f);
    self.caches.insert(q::PATH, HashMap::<q::Input, q::Output>::new());
}

run(q, i) {
    let f = self.fns.get(q::PATH).unwrap();
    let mut cache = self.caches.get(q::PATH);
    if Some(c) = cache.get(i) {
        let newest_dep = c.dependencies.map(|(f, k)| self.caches.get(f)?.get(i)?.version).max()
        if c.version >= newest_dep {
            return c.value;
        }
    }
    let out = f(i);
    cache.insert(i, out);
    out
}

struct Driver {
    cache: yeter::Cache,
}

impl WithCache for Driver {
    fn cache(&self) -> &yeter::Cache {
        &self.cache
    }
}

impl Driver {
    fn new() -> Self {
        Self { cache: yeter::Cache::empty(), }
    }

    yeter::query!(type_check (node_name: &str) -> Option<Type> {
        let node = self.get_node(node_name)?;
        for equation in node.equations() {
            self.type_check_equation(equation);
        }
    });
}

fn type_check(&self, node_name: &str) -> Option<Type> {
    let key = node_name;
    let id = "rustre_core::typing::type_check";
    let current_cache = self.cache().entry(id).or_with(|| Mutex::new(HashMap::new()));

    let version = self.cache().version(id, key);
    if !current_cache.contains(key) || self.cache().dependencies(id, key).any(|x| is_outdated(version, x)) {
        self.cache().push(id, key); // records dependencies of calls already on the stack

        let result = {
            let node = self.get_node(node_name)?;
            for equation in node.equations() {
                self.type_check_equation(equation);
            }
        };

        current_cache.insert(key, result);
        self.cache().pop();
    }
   
    current_cache.get(key).unwrap()
}

fn is_outdated(version, x) = version < x.version()
```