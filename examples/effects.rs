mod string {
    #[yeter::query]
    pub fn len(db: &yeter::Database, input: String) -> usize {
        if input.is_empty() {
            db.do_effect("Empty string");
        }
        input.len()
    }
}

fn main() {
    let db = yeter::Database::new();
    let len1 = string::len(&db, "".into());
    for msg in db.effect::<&'static str>() {
        println!("EFFECT [1]: {}", msg);
    }
    let len2 = string::len(&db, "".into());
    for msg in db.effect::<&'static str>() {
        println!("EFFECT [2]: {}", msg);
    }
    let len3 = string::len(&db, "aaaa".into());
    for msg in db.effect::<&'static str>() {
        println!("EFFECT [3]: {}", msg);
    }
    assert_eq!(len1, len2);
    assert_eq!(*len3, 4);
}