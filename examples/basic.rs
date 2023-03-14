yeter::queries! {
    string {
        len : String : usize
    }
}

fn main() {
    let mut db = yeter::Database::new();
    db.register::<_, string::len>(|_db, name| {
        dbg!(name.len())
    });
    let len1 = string::len(&db, "hello".into());
    let len2 = string::len(&db, "hello".into());
    let len3 = string::len(&db, "world".into());
    assert_eq!(len1, len2);
    assert_eq!(len1, len3);
}