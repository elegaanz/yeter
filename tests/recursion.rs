use yeter::Database;

// Checks regression in https://github.com/elegaanz/yeter/issues/5 (2.)
// The `fib` calls inside the query would collide with internal generated code.

#[yeter::query]
fn fib(db: &Database, idx: u64) -> u64 {
    match idx {
        0 => 0,
        1 => 1,
        idx => *fib(db, idx - 1) + *fib(db, idx - 2),
    }
}

#[test]
fn compiles() {}
