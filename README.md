# Yéter — KISS incremental computation framework

Yeter is a crate that memoizes pure function outputs,
and only recompute them when neeed. It is similar to
`salsa`, but is using a dynamic approach rather than
static definitions.

For this reason (and also because a lot less effort was
put into it than in salsa), it is probably performing
worse, and salsa should probably be preferred for any
serious project that doesn't require dynamic definitions.

## Overview

Yéter is built around pure functions, that are called
**queries**, and a cache that we call a **database**.

To create an empty database, you can use `Database::new`.

```rust
let mut db = yeter::Database::new();
```

You can define queries with the `query!` macro. Note
that this macro only defines the name and type of a query,
allowing you to dynamically re-define its behavior.

```rust
// Parameters : name, input type, output type
query!(sum, Vec<usize>, usize);
```

The function `Database::register` can be used to let
a database know about a query.

```rust
// sum::Query is a type defined by the query! macro
db.register::<sum::Query>(|_db, list| {
    list.iter().sum()
});
```

A query can then be executed with `Database::run`, or even
better, with the function that the `query!` macro generates.

```rust
let result = sum::query(db, vec![1, 2, 3]);
assert_eq!(*result, 6);
```

The result of each call to a query is cached. It will only
be recomputed if the query was redefined, or if one of the
query it depends on was recomputed since last time.

## Notes

- Queries are assumed to be pure and only depend on their input
  and on the database. Any file system or network access breaks
  this assumption, and will make Yéter fail to do its job properly
- What is called "inputs" in salsa is just a query with a `()` input
  and no dependency on other queries. Instead of calling a `set_*`
  method, you redefine the query every time you want to set a new value.