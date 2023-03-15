#[yeter::query]
fn list(_db: &yeter::Database, _: ()) -> Vec<usize> {
    vec![1, 2, 3]
}

#[yeter::query]
fn sum(db: &yeter::Database, _: ()) -> usize {
    list(db, ()).iter().sum()
}

fn main() {
    let mut db = yeter::Database::new();

    db.register_impl::<list>();
    db.register_impl::<sum>();

    assert_eq!(*sum(&db, ()), 6);

    db.register::<_, list>(|_db, ()| vec![]);

    assert_eq!(*sum(&db, ()), 0);
}