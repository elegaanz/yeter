#[yeter::query]
fn list(_db: &yeter::Database) -> Option<Vec<usize>>;

#[yeter::query]
fn sum(db: &yeter::Database) -> usize {
    let list = list(db);
    list.as_ref().as_deref().unwrap_or_default().iter().sum()
}

fn main() {
    let db = yeter::Database::new();

    db.set::<list>((), Some(vec![1, 2, 3]));
    assert_eq!(*sum(&db), 6);

    db.set::<list>((), Some(vec![]));
    assert_eq!(*sum(&db), 0);
}