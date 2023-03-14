use yeter::query;

query!(list, (), Vec<usize>);
query!(sum, (), usize);

fn main() {
    let mut db = yeter::Database::new();
    db.register::<_, list>(|_db, ()| {
        vec![1, 2, 3]
    });
    db.register::<_, sum>(|db, ()| {
        list(db, ()).iter().sum()
    });
    assert_eq!(*sum(&db, ()), 6);

    db.register::<_, list>(|_db, ()| {
        vec![]
    });
    assert_eq!(*sum(&db, ()), 0);
}