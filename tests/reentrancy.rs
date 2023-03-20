use yeter::Database;

#[yeter::query]
fn a(db: &Database) -> usize {
    *depends_on_a(db) + 1
}

#[yeter::query]
fn depends_on_a(db: &Database) -> usize {
    *a(db) - 1
}

// a and b depend on one another, forming a cycle
// yeter should detect this

#[test]
#[should_panic(expected = "Cycle")]
fn disallow() {
    let mut db = Database::new();
    db.register_impl::<a>();
    db.register_impl::<depends_on_a>();

    // This cannot be evaluated
    dbg!(a(&db));
}

#[yeter::query]
fn fib(db: &Database, idx: u64) -> u64 {
    match idx {
        0 => 0,
        1 => 1,
        idx => *fib_r(db, idx - 1) + *fib_r(db, idx - 2),
    }
}

// FIXME remove (https://github.com/elegaanz/yeter/issues/5)
#[inline]
fn fib_r(db: &Database, idx: u64) -> std::rc::Rc<u64> {
    fib(db, idx)
}

#[test]
fn allow_if_different_input() {
    let mut db = Database::new();
    db.register_impl::<fib>();
    assert_eq!(*fib(&db, 15), 610);
}
