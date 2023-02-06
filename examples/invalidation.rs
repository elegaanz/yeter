use yeter::query;

query!(list, (), Vec<usize>);
query!(sum, (), usize);

fn main() {
    let mut db = yeter::Database::new();
    db.register::<list::Query>(|_db, ()| {
        vec![1, 2, 3]
    });
    db.register::<sum::Query>(|db, ()| {
        list::query(db, ()).iter().sum()
    });
    assert_eq!(*sum::query(&db, ()), 6);

    db.register::<list::Query>(|_db, ()| {
        vec![]
    });
    assert_eq!(*sum::query(&db, ()), 0);
}