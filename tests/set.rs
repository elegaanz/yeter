use yeter::Database;

#[yeter::query]
fn setable_query(db: &Database, input: String) -> Option<String>;

#[yeter::query]
fn id(db: &Database, input: String) -> String {
    let rc = setable_query(db, input);
    Option::as_ref(&rc).unwrap().to_string()
}

#[test]
fn simple_1_keyed() {
    let db = Database::new();
    db.set::<setable_query>(("Bob".into(),), Some("123".into()));
    assert!(String::eq(&id(&db, "Bob".into()), "123"));
}

#[yeter::query]
fn pixel(db: &Database, x: usize, y: usize) -> Option<u8>;

#[test]
fn simple_2_keyed() {
    let db = Database::new();
    db.set::<pixel>((3, 3), Some(3));
    db.set::<pixel>((3, 4), Some(4));
    assert_eq!(*pixel(&db, 0, 0), None);
    assert_eq!(*pixel(&db, 3, 4), Some(4));
    assert_eq!(*pixel(&db, 3, 3), Some(3));
}
